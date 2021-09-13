#!/usr/bin/env bash

wget -nc https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
chmod +x linuxdeploy-x86_64.AppImage

# Notes for future generations:
# --appdir: specifies linuxdeploy's output directory, where to create the AppDir.
# --executable: specifies the executable file.
# --icon-file and --desktop-file are self-explanatory.
# --output appimage: specifies that an AppImage should be generated.

./linuxdeploy-x86_64.AppImage \
  --appdir NetCanv-AppDir \
  --executable target/release/netcanv \
  --icon-file resources/netcanv.png \
  --desktop-file resources/netcanv.desktop \
  --output appimage

./linuxdeploy-x86_64.AppImage \
  --appdir NetCanv-Matchmaker-AppDir \
  --executable target/release/netcanv-matchmaker \
  --icon-file resources/netcanv.png \
  --desktop-file resources/netcanv-matchmaker.desktop \
  --output appimage

mkdir appimages
mv NetCanv-*.AppImage NetCanv_Matchmaker-*.AppImage appimages
rm -r NetCanv*-AppDir
