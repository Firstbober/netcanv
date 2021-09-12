## WallhackD

### Changes
- Pasting images
- Custom Room ID with WallhackD Matchmaker
- Command line options
- Headless client and server
- Save canvas without UI
- Filling out username, roomid and matchmaker address using cli
- Username length raised to 24 characters
- Shows number of loaded chunks
- Teleport around using handy inputs
- RGB Color input
- Chat between WallhackD clients
- Find player real life location
- Themes based on selected colors (you can change color in config)

# NetCanv

This repository hosts source code for my YouTube tutorial series for a
multiplayer Paint app.

The series is in Polish (!) and can be found on
[YouTube](https://www.youtube.com/playlist?list=PL1Hg-PZUNFkeRdErHKx3Z7IwhJNgij3bJ).

Individual episodes:

1. [Introduction](https://www.youtube.com/watch?v=ZeSXVgjrivY)
2. [Drawing and GUI](https://www.youtube.com/watch?v=MVEILFrPKnY)
3. [Refactoring and ∞](https://www.youtube.com/watch?v=mECVCb87sAQ)
4. Networking – coming soon

Again, note that the tutorials are in Polish. I do plan on making English
subtitles available at some point, though.

## Purpose

The main purpose of this tutorial series is to show how to build a desktop app
using Rust and Skia, together with peer-to-peer communication for realtime
collaboration.

I generally don't like explaining every small detail in my tutorials. I'd rather
showcase the cool and interesting parts about the development process. So don't
consider this as a general Rust application development tutorial – treat it more
like a devlog with some educational, comedic, and artistic value sprinkled
over it.

## Compiling

Should be as simple as:

```sh
$ cargo build --release
# or, if you just want to run the app:
$ cargo run --release
```

Thanks, mature ecosystem!

### Matchmaker

NetCanv assumes that you have your own matchmaker up: currently the default value
in the matchmaker textbox is `localhost:62137`, for easy testing.

To run the matchmaker, simply do:
```sh
$ cd netcanv-matchmaker
$ cargo run --release
```

This will allow you to host and join new rooms locally.
