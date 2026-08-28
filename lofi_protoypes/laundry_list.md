Small Tasks:
- Back and forward buttons should take into account where a page has been scrolled to and what's in the search box so that they can be restored (still not working)
- If a track is shown as playing in a list, the pause icon it has should play/pause the song rather than start from the beginning. The exception is a playlist where the track is treated as separate so it shouldn't even light up as playing if the song is being played from another source (working but this should apply to the queue as well)
- Scrolling is noticably not smooth going through songs, feels like the "framerate" is capped
- Items in a playlist should be able to be dragged and dropped into a different order, just like the queue
- Top bar on KDE should have priority over the content of the rest of the app. That is to say, the scrollbar should not be crossing over the window control buttons, the side panels should not move the buttons, and the red outline for close should be flush with the corner of the application

Medium Tasks:
- Duplicate song handling - same song gets collapsed into one, combining metadata in the program's database, file that is played back is the one with the highest quality (bitrate, sample rate, file type)
- Settings page - Theme, fade pause/play, keep reverb on pause, library management, output device override select
- Expanded EQ controls brings up a dialogue modal pop-up that is a clone of Logic Pro's EQ controls in this application's style
- Home page showing recently played, archive mix, most played playlists, and simple recommendations

Large Tasks:
- Playlists can have custom crossfades between each song. These allow you to start fading anywhere in either song, sample a section of a song (or even import an audio file) to place at certain places on a timeline during the crossfade (including looping), add select DJ Mixer effects to any sample or just a specific part of one of the songs, and finally allow playlists to be exported as an MP3, FLAC, or WAV file (bounce the mix) that retains all of this and other settings.
- In the proper queue view, have shaders that create visuals live from the song in the background. These are any combinations of colours, patterns, shapes, moving effects (pulsing, bouncing, squish and squash), and even lyric detection (or pulled from music database if easier) to display words dynamically in the background popping up in random locations. It would be cool if this was 3D visuals but 2D would probably be more reasonable.
- Support for remote libraries (navidrome and Jellyfin) and merging/syncing with local libraries 
- Mobile port (Android mainly, iOS if possible)
- Plugin support for other users to easily add features. Will need an API to access backend features and a way to add custom vue items into the interface. For example, a plugin that adds YT-DLP functionality that allows you to download songs and playlists from YouTube links. Or a plugin that adds a transcoding option in the context menu that uses ffmpeg. Or a plugin that adds a new effect into the DJ Mixer
