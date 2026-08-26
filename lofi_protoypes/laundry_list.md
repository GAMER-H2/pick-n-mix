Small Tasks:
- Playing a new track seems to make an audible clip briefly just and moment the track is played. Not entirely sure if this is the program or my audio device but I only notice it on this program so my suspicion is the former.
- Full-screen queue view slide in is faster now but still quite choppy, a simple quick fade in and out over the current main page would probably be better now
- Search on library page is non-functional. It should search the current view (songs, albums, artists)
- Loaded over 1100+ songs into the library now to test scale capabilities of the application. Found scrolling through the queue, song list, album list, and artist list to be a bit laggy now. Figure out some changes with loading the content (I imagine the images are the main culprit) in a different dynamic way to cut down on resource usage to make the interface smooth as possible regardless of the number of tracks in the program

Medium Tasks:
- Duplicate song handling - same song gets collapsed into one, combining metadata in the program's database, file that is played back is the one with the highest quality (bitrate, sample rate, file type)
- Settings page - Theme, fade pause/play, keep reverb on pause, library management, output device override select
- Expanded EQ controls brings up a dialogue modal pop-up that is a clone of Logic Pro's EQ controls in this application's style
- Crossfade global controls in DJ Mixer - simple slider for crossfade length in pop-up DJ Mixer, graph controls for global DJ Mixer that shows the keyframes of each song to manipulate
- Home page showing recently played, archive mix, most played playlists, and simple recommendations

Large Tasks:
- Playlists can have custom crossfades between each song. These allow you to start fading anywhere in either song, sample a section of a song (or even import an audio file) to place at certain places on a timeline during the crossfade (including looping), add select DJ Mixer effects to any sample or just a specific part of one of the songs, and finally allow playlists to be exported as an MP3, FLAC, or WAV file (bounce the mix) that retains all of this and other settings.
- In the proper queue view, have shaders that create visuals live from the song in the background. These are any combinations of colours, patterns, shapes, moving effects (pulsing, bouncing, squish and squash), and even lyric detection (or pulled from music database if easier) to display words dynamically in the background popping up in random locations. It would be cool if this was 3D visuals but 2D would probably be more reasonable.
- Support for remote libraries (navidrome and Jellyfin) and merging/syncing with local libraries 
- Mobile port (Android mainly, iOS if possible)
- Plugin support for other users to easily add features. Will need an API to access backend features and a way to add custom vue items into the interface. For example, a plugin that adds YT-DLP functionality that allows you to download songs and playlists from YouTube links. Or a plugin that adds a transcoding option in the context menu that uses ffmpeg. Or a plugin that adds a new effect into the DJ Mixer
