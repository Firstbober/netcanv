// the netcanv matchmaker server.
// keeps track of open rooms and exchanges addresses between hosts and their clients

use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::error;
use std::io::{BufReader, BufWriter, Write};
use std::net::{AddrParseError, SocketAddr, TcpListener, TcpStream};
use std::ops::Deref;
use std::sync::{Arc, Mutex, Weak};
use tungstenite::{Message, WebSocket, accept};

use netcanv_protocol::matchmaker::*;
use thiserror::Error;

/// Maximum possible room ID. This can be raised, if IDs ever run out.
const MAX_ROOM_ID: u32 = 9999;

/// A TCP stream and websocket packed into one thread-safe struct for
/// convenience.
struct BufStream {
   stream: TcpStream,
   websocket: Mutex<WebSocket<TcpStream>>,
}

impl BufStream {
   /// Creates a new BufStream from a TcpStream.
   fn new(stream: TcpStream) -> Result<Self, Error> {
      const MEGABYTE: usize = 1024 * 1024;

      Ok(Self {
         websocket: Mutex::new(accept(stream.try_clone()?).unwrap()),
         stream,
      })
   }
}

impl Deref for BufStream {
   type Target = TcpStream;

   fn deref(&self) -> &Self::Target {
      &self.stream
   }
}

/// A room containing the host and weak references to relay clients connected to the room.
#[derive(Clone)]
struct Room {
   host: Arc<BufStream>,
   clients: Vec<Weak<BufStream>>,
   id: u32,
}

/// The matchmaker state, usually passed around behind an Arc<Mutex<T>>.
struct Matchmaker {
   /// The rooms available on the matchmaker server. Each room is available behind an
   /// Arc<Mutex<T>>, so that accessing a room does not require locking the matchmaker mutex.
   rooms: HashMap<u32, Arc<Mutex<Room>>>,
   /// A mapping from host addresses to their room IDs.
   host_rooms: HashMap<SocketAddr, u32>,
   /// A mapping from relay client addresses to their room IDs.
   relay_clients: HashMap<SocketAddr, u32>,
}

/// A runtime error.
#[derive(Debug, Error)]
enum Error {
   #[error("I/O error: {0}")]
   Io(#[from] std::io::Error),
   #[error("Unrecognized or unimplemented packet")]
   InvalidPacket,
   #[error("(De)serialization error: {0}")]
   Serialize(#[from] bincode::Error),
   #[error("Invalid address: {0}")]
   InvalidAddr(#[from] AddrParseError),
}

impl Matchmaker {
   /// Creates a new matchmaker.
   fn new() -> Self {
      Self {
         rooms: HashMap::new(),
         host_rooms: HashMap::new(),
         relay_clients: HashMap::new(),
      }
   }

   /// Serializes a packet into the stream.
   fn send_packet(stream: &BufStream, packet: &Packet) -> Result<(), Error> {
      match &packet {
         Packet::Relayed(..) => (),
         packet => eprintln!("- sending packet {} -> {:?}", stream.peer_addr()?, packet),
      }

      let ser_res = bincode::serialize(packet);

      if ser_res.is_err() {
         return Err(Error::Serialize(ser_res.err().unwrap()));
      }

      stream.websocket.lock().unwrap().write_message(Message::Binary(ser_res.unwrap())).unwrap();

      Ok(())
   }

   /// Sends an error packet into the stream.
   fn send_error(stream: &BufStream, error: &str) -> Result<(), Error> {
      Self::send_packet(stream, &error_packet(error))
   }

   /// Searches for a free room ID by rolling a dice 50 times until an ID is found.
   /// If an ID cannot be found, None is returned and the requesting client is expected to ask for
   /// a free room again.
   fn find_free_room_id(&self) -> Option<u32> {
      use rand::Rng;
      let mut rng = rand::thread_rng();
      for _ in 1..50 {
         let id = rng.gen_range(0..=MAX_ROOM_ID);
         if !self.rooms.contains_key(&id) {
            return Some(id);
         }
      }
      None
   }

   /// Packet::Host handler. Searches for a free room ID, and sends it to the requesting client.
   fn host(
      mm: Arc<Mutex<Self>>,
      peer_addr: SocketAddr,
      stream: Arc<BufStream>,
   ) -> Result<(), Error> {
      let mut mm = mm.lock().unwrap();
      match mm.find_free_room_id() {
         Some(room_id) => {
            let room = Room {
               host: stream.clone(),
               clients: Vec::new(),
               id: room_id,
            };
            {
               mm.rooms.insert(room_id, Arc::new(Mutex::new(room)));
               mm.host_rooms.insert(peer_addr, room_id);
            }
            drop(mm);
            Self::send_packet(&stream, &Packet::RoomId(room_id))?;
         }
         None => Self::send_error(&stream, "Could not find any more free rooms. Try again")?,
      }
      Ok(())
   }

   /// Packet::GetHost handler. Finds the host with the given ID, and exchanges addresses between
   /// the client and the host.
   fn join(mm: Arc<Mutex<Self>>, stream: &BufStream, room_id: u32) -> Result<(), Error> {
      let mm = mm.lock().unwrap();
      let room = match mm.rooms.get(&room_id) {
         Some(room) => room,
         None => {
            Self::send_error(
               stream,
               "No room found with the given ID. Check whether you spelled the ID correctly",
            )?;
            return Ok(());
         }
      }
      .lock()
      .unwrap();
      let client_addr = stream.peer_addr()?;
      let host_addr = room.host.peer_addr()?;
      Self::send_packet(&room.host, &Packet::ClientAddress(client_addr))?;
      Self::send_packet(stream, &Packet::HostAddress(host_addr))
   }

   /// Adds a relay client to the matchmaker.
   fn add_relay(
      mm: Arc<Mutex<Self>>,
      stream: Arc<BufStream>,
      host_addr: Option<SocketAddr>,
   ) -> Result<(), Error> {
      let peer_addr = stream.peer_addr().unwrap();
      eprintln!("- relay requested from {}", peer_addr);

      let host_addr: SocketAddr = host_addr.unwrap_or(peer_addr);
      let mut mm = mm.lock().unwrap();
      let room_id: u32;
      match mm.host_rooms.get(&host_addr) {
         Some(id) => room_id = *id,
         None => {
            Self::send_error(&stream, "The host seems to have disconnected")?;
            return Ok(());
         }
      }
      mm.relay_clients.insert(peer_addr, room_id);
      mm.rooms.get_mut(&room_id).unwrap().lock().unwrap().clients.push(Arc::downgrade(&stream));

      // Don't forget to notify the requester that the relay is now ready.
      Self::send_packet(&stream, &Packet::Relayed(peer_addr, vec![]))?;

      Ok(())
   }

   /// Relays a packet to a specific relay client in the sender's room, or all relay clients in
   /// that room, depending on whether `to` is `Some` or `None`.
   fn relay(
      mm: Arc<Mutex<Self>>,
      addr: SocketAddr,
      stream: &Arc<BufStream>,
      to: Option<SocketAddr>,
      data: Vec<u8>, // Vec because it's moved out of the Relay packet
   ) -> Result<(), Error> {
      eprintln!("relaying packet (size: {} KiB)", data.len() as f32 / 1024.0);
      let mut mm = mm.lock().unwrap();
      let room_id = match mm.relay_clients.get(&addr) {
         Some(id) => *id,
         None => {
            Self::send_error(stream, "Only relay clients may send Relay packets")?;
            return Ok(());
         }
      };
      match mm.rooms.get_mut(&room_id) {
         Some(room) => {
            let mut room = room.lock().unwrap().clone();
            drop(mm);
            let mut nclients = 0;
            room.clients.retain(|client| client.upgrade().is_some());
            let packet = Packet::Relayed(addr, data);
            for client in &room.clients {
               let client = &client.upgrade().unwrap();
               if !Arc::ptr_eq(client, stream) {
                  if let Some(addr) = to {
                     if client.peer_addr()? != addr {
                        continue;
                     }
                  }
                  Self::send_packet(client, &packet)?;
                  nclients += 1;
               }
            }
            eprintln!("- relayed from {} to {} clients", addr, nclients);
         }
         None => {
            Self::send_error(stream, "The host seems to have disconnected")?;
            return Ok(());
         }
      }

      Ok(())
   }

   /// Dispatch point for all the different functions for handling packets.
   fn incoming_packet(
      mm: Arc<Mutex<Self>>,
      peer_addr: SocketAddr,
      stream: Arc<BufStream>,
      packet: Packet,
   ) -> Result<(), Error> {
      match &packet {
         Packet::Relay(..) => (),
         packet => eprintln!("- incoming packet: {:?}", packet),
      }
      match packet {
         Packet::Host => Self::host(mm, peer_addr, stream),
         Packet::GetHost(room_id) => Self::join(mm, &stream, room_id),
         Packet::RequestRelay(host_addr) => Self::add_relay(mm, stream, host_addr),
         Packet::Relay(to, data) => Self::relay(mm, peer_addr, &stream, to, data),
         _ => {
            eprintln!("! error/invalid packet: {:?}", packet);
            Err(Error::InvalidPacket)
         }
      }
   }

   /// Disconnects a client gracefully by removing all references to it inside of the matchmaker.
   fn disconnect(&mut self, peer_addr: SocketAddr, stream: &Arc<BufStream>) -> Result<(), Error> {
      if let Some(room_id) = self.host_rooms.remove(&peer_addr) {
         self.rooms.remove(&room_id);
      }
      if let Some(room_id) = self.relay_clients.remove(&peer_addr) {
         if let Some(room) = self.rooms.get_mut(&room_id) {
            let room = room.lock().unwrap();
            for client in &room.clients {
               let client = client.upgrade();
               if client.is_none() {
                  continue;
               }
               let client = client.unwrap();
               if Arc::ptr_eq(&client, stream) {
                  continue;
               }
               let _ = Self::send_packet(&client, &Packet::Disconnected(peer_addr));
            }
         }
      }
      Ok(())
   }

   /// Starts a new client handler thread that reads packets from the client and deserializes them,
   /// then passing them into the incoming_packet function.
   fn start_client_thread(mm: Arc<Mutex<Self>>, tcp_stream: TcpStream) -> Result<(), Error> {
      let peer_addr = tcp_stream.peer_addr()?;
      let stream = Arc::new(BufStream::new(tcp_stream)?);

      eprintln!("* mornin' mr. {}", peer_addr);
      let _ = std::thread::spawn(move || {
         let mut running = true;
         while running {
            let mut buf = [0; 1];
            if let Ok(n) = stream.peek(&mut buf) {
               if n == 0 {
                  let _ = mm.lock().unwrap().disconnect(peer_addr, &stream).or_else(
                     |error| -> Result<_, ()> {
                        eprintln!("! error/while disconnecting {}: {}", peer_addr, error);
                        Ok(())
                     },
                  );
                  break;
               }
            }

            let msg = stream.websocket.lock().unwrap().read_message().unwrap().into_data();

            let _ = bincode::deserialize(&msg) // what
               .map_err(|error| {
                  running = false;
                  Error::Serialize(error)
               })
               .and_then(|decoded| {
                  Self::incoming_packet(mm.clone(), peer_addr, stream.clone(), decoded)
               })
               .or_else(|error| -> Result<_, ()> {
                  eprintln!("! error/packet decode from {}: {}", peer_addr, error);
                  Ok(())
               });
         }
         eprintln!("* bye bye mr. {} it was nice to see ya", peer_addr);
      });
      Ok(())
   }
}

fn main() -> Result<(), Box<dyn error::Error>> {
   let mut port: u16 = DEFAULT_PORT;
   let mut args = std::env::args();
   args.next();
   if let Some(port_str) = args.next() {
      port = port_str.parse()?;
   }

   eprintln!("NetCanv Matchmaker: starting on port {}", port);

   // 127.0.0.1 didn't want to work for some reason
   let localhost = SocketAddr::from(([0, 0, 0, 0], port));
   let listener = TcpListener::bind(localhost)?;

   let state = Arc::new(Mutex::new(Matchmaker::new()));

   eprintln!("Listening for incoming connections");

   for connection in listener.incoming() {
      connection
         .map_err(|error| Error::from(error))
         .and_then(|stream| {
            stream.set_nodelay(true)?;
            Matchmaker::start_client_thread(state.clone(), stream)
         })
         .or_else(|error| -> Result<_, ()> {
            eprintln!("! error/connect: {}", error);
            Ok(())
         })
         .unwrap(); // silence, compiler
   }

   Ok(())
}
