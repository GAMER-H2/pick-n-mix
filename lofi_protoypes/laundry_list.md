Small Tasks:
- Add presets for advanced EQ
- Fix the effects knobs not having their their pointer facing the bar around them
- Shift-clicking the mixer button doesn't close the advanced mixer
- Context menu on all home page items
- Atmospheres vinyl crackle needs re-encoding to 48kHz
- Atmospheres shaders need more refinement
- Atmospheres volume knobs need a more dedicated place to not jerk around the mixer pop-up

Medium Tasks:
- Duplicate song handling - (working but needs some refinement as there are a lot of true negatives)

Large Tasks:
- Playlists can have custom crossfades between each song. These allow you to start fading anywhere in either song, sample a section of a song (or even import an audio file) to place at certain places on a timeline during the crossfade (including looping), add select DJ Mixer effects to any sample or just a specific part of one of the songs, and finally allow playlists to be exported as an MP3, FLAC, or WAV file (bounce the mix) that retains all of this and other settings.
- In the proper queue view, have shaders that create visuals live from the song in the background. These are any combinations of colours, patterns, shapes, moving effects (pulsing, bouncing, squish and squash), and even lyric detection (or pulled from music database if easier) to display words dynamically in the background popping up in random locations. It would be cool if this was 3D visuals but 2D would probably be more reasonable.
- Support for remote libraries (navidrome and Jellyfin) and merging/syncing with local libraries 
- Mobile port (Android mainly, iOS if possible)
- Plugin support for other users to easily add features. Will need an API to access backend features and a way to add custom vue items into the interface. For example, a plugin that adds YT-DLP functionality that allows you to download songs and playlists from YouTube links. Or a plugin that adds a transcoding option in the context menu that uses ffmpeg. Or a plugin that adds a new effect into the DJ Mixer
