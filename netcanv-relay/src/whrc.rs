use std::{sync::Arc, net::SocketAddr};

use netcanv_protocol::relay::{Packet, self, RoomId};
use tokio::sync::Mutex;

use crate::send_packet;

pub async fn whrc_custom_id_host(
   write: &Arc<Mutex<crate::Sink>>,
   address: SocketAddr,
   state: &mut crate::State,
	room_id: RoomId
) -> anyhow::Result<()> {
   let peer_id = if let Some(id) = state.peers.allocate_peer_id(Arc::clone(write), address) {
      id
   } else {
      send_packet(write, Packet::Error(relay::Error::NoFreePeerIDs)).await?;
      anyhow::bail!("no more free peer IDs");
   };

	if state.rooms.host_id(room_id).is_some() {
		send_packet(write, Packet::Error(relay::Error::NoFreeRooms)).await?;
		anyhow::bail!("Room ID {} is in use", room_id);
	}

	if state.rooms.occupied_room_ids.insert(room_id) {
		state.rooms.room_clients.insert(room_id, Vec::new());
	}

   state.rooms.make_host(room_id, peer_id);
   state.rooms.join_room(peer_id, room_id);
   send_packet(write, Packet::RoomCreated(room_id, peer_id)).await?;

   Ok(())
}

#[macro_export]
macro_rules! whrc_handle_packets {
   ($packet: expr, $write: expr, $address: expr, $state: expr) => {
		match $packet {
			Packet::WHRCHostCustomId(room_id) => whrc::whrc_custom_id_host($write, $address, &mut *$state.lock().await, room_id).await?,
			_ => {}
		}
   };
}