<table>
  <tr>
    <td width="112" valign="middle">
      <img src="src-tauri/icons/icon.png" alt="Pick n Mix logo" width="96" />
    </td>
    <td valign="middle">
      <h1>Pick n Mix</h1>
      <p>A local music player with a live DJ mixer: pitch, EQ, reverb, delay, lo-fi crushing and ambience beds applied to your own files as they play.</p>
    </td>
  </tr>
</table>

# Features
- **Local Music Playback** — point the app at a folder and it reads tags, artwork and ReplayGain from the files themselves. Playback your audio files at full quality, with no transcoding or cloud upload. Supported formats include MP3, FLAC, WAV, OGG, AAC and M4A.
- **Live DJ Mixer** — pitch, EQ, reverb, delay, lo-fi crushing, crossfade, panning, and ambience beds applied to your own files as they play. The mixer is global, per-playlist, or per-song within a playlist, and each layer inherits from the one above it.
- **Playlist Master Mixer** — customise your playlist extensively with the master mixer giving you a mini-DAW like interface. Move tracks round like audio clips on a timeline, add custom audio files, edit the audio levels with keyframes, and apply effects to the whole playlist or individual audio blocks. Save your mixes as new playlists, or export them as audio files.
- **High Performance** - built on Rust and Tauri, with a modern Vue 3 frontend. The audio engine is written in Rust for low-latency, high-performance audio processing. The Vue 3 frontend has many optimisations to make the handling of large libraries and playlists smooth and responsive. The app will look nicely native on Windows, macOS and Linux, and is designed to be lightweight and efficient.
- ***[Coming Soon]*** **Remote Libraries** — support for Navidrome and Jellyfin, with merging and syncing with local libraries. Play your music from a remote source, or sync it to your local library for offline playback.
- ***[Coming Soon]*** **Music Visualisations** — shaders that create visuals live from the song. These are any combinations of colours, patterns, shapes, moving effects, and even lyric detection to display words dynamically in the background popping up in random locations.

# Try it Out
## Download
Not yet available on any package mangers, please download for your OS from the [releases page](https://github.com/GAMER-H2/pick-n-mix/releases).

## Build
### Short Version
```bash
git clone https://github.com/GAMER-H2/pick-n-mix.git
cd pick-n-mix
npm install
npm run tauri build
```
### Long Version
#### General Prerequisites
- [NPM & Node.js](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

#### Linux Packages Prerequisites
##### Debian / Ubuntu

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  pkg-config \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libxdo-dev \
  libssl-dev \
  libasound2-dev
```

##### Fedora

```bash
sudo dnf install -y \
  gcc \
  gcc-c++ \
  make \
  pkgconf-pkg-config \
  gtk3-devel \
  webkit2gtk4.1-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel \
  openssl-devel \
  alsa-lib-devel
```

##### Arch Linux

```bash
sudo pacman -Syu --needed \
  base-devel \
  pkgconf \
  gtk3 \
  webkit2gtk-4.1 \
  libappindicator-gtk3 \
  librsvg \
  xdotool \
  openssl \
  alsa-lib
```

##### NixOS

Use a development shell which makes the required headers and `pkg-config`
metadata available to the build:

```bash
nix-shell -p \
  pkg-config \
  gcc \
  gtk3 \
  webkitgtk_4_1 \
  libayatana-appindicator \
  librsvg \
  libxdo \
  openssl \
  alsa-lib
```

Run `npm install` and `npm run tauri build` from that shell. For a persistent
setup, put the same packages in a `devShell` in your flake or `shell.nix`.

##### openSUSE

```bash
sudo zypper install -y -t pattern devel_basis
sudo zypper install -y \
  pkg-config \
  gtk3-devel \
  webkit2gtk3-devel \
  libappindicator3-devel \
  librsvg-devel \
  libXdo-devel \
  libopenssl-devel \
  alsa-devel
```
#### Building
1. Clone the repository:
```bash
git clone https://github.com/GAMER-H2/pick-n-mix.git
```
2. Change into the project directory:
```bash
cd pick-n-mix
```
3. Install dependencies:
```bash
npm install
```
4. Build the application (binaries will be in `src-tauri/target/release`):
```bash
npm run tauri build
```
#### AppImage on rolling-release distributions
Tauri's bundled linuxdeploy uses an old `strip` (binutils 2.35) that cannot
handle the modern `SHT_RELR` (`.relr.dyn`) sections in current Arch/CachyOS
libraries, so the AppImage has to be built unstripped:
```bash
npm run build:appimage   # NO_STRIP=1 tauri build --bundles appimage
```
`NO_STRIP=1` applies to every bundle format, so use it for a full build too:
```bash
NO_STRIP=1 npm run tauri build
```
The released AppImages are built the same way; see the Linux job in
`.github/workflows/build.yml`.

An AppImage carries the WebKitGTK it was built against, which on a
rolling-release host is far older than the graphics drivers it then has to
talk to. When the two disagree the webview composites nothing, and because
the window is transparent that shows up as an *invisible* window rather than
a blank one. The app detects an AppImage run and falls back to WebKit's own
compositing (`WEBKIT_DISABLE_DMABUF_RENDERER=1`). To get the accelerated path
back on a host where it works, set it yourself:
```bash
WEBKIT_DISABLE_DMABUF_RENDERER=0 ./Pick-n-Mix-*.AppImage
```
A native package (`.deb`, `.rpm`, or a local `npm run tauri build`) uses the
distribution's own WebKit and is unaffected either way.
