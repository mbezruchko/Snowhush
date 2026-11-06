Snowhush rain sounds
====================

Drop audio files in this folder, then run ./build.sh (or build in Xcode).
build.sh copies this folder into Snowhush.app/Contents/Resources/Sounds.
Accepted extensions: .wav .aiff .aif .m4a .mp3 .caf .aac
Non-audio files (like this one) are ignored, and only files directly in
this folder are read — subfolders are ignored, so spare recordings can be
kept in one. The app reads the folder at launch, so restart Snowhush after
adding, removing or replacing sounds.

--- The simple setup: one combined file ---------------------------------

Put a single rain-and-thunder file in here, under any name. It is looped
forever while Rain mode is on. The volume slider, the Rain Sounds toggle,
Intensity and the sleep/lock muting all apply to it, and the thunder baked
into the file comes round again on each loop. With no separate thunder
clips the "Thunder" menu item stays hidden.

--- The fuller setup: one rain bed + separate thunder -------------------

  rain.wav             ONE rain recording, looped forever.
  thunder-1.wav        One-shot claps. Any number may be added; one is
  thunder-2.wav        picked at random every 20-75 seconds (more often
  thunder-3.wav        in heavy rain, less in a light shower). Anything
                       whose name starts with "thunder" counts.

Keep the bed to a single, consistent rain recording. Mixing several
different rain recordings in sequence is easy to hear and breaks the flow;
if you want to compare a few, keep them in a subfolder and copy one over
rain.wav at a time.

With separate thunder files present, the menu bar menu gains a "Thunder"
on/off toggle alongside "Rain Sounds" and the volume slider.

--- Loop quality ---------------------------------------------------------

WAV, AIFF or CAF is best: MP3 often carries a fraction of a second of
encoder padding at the start and end, which can be audible at the loop
point. If you hear a seam, convert the file.

--- Menu -----------------------------------------------------------------

With Rain mode selected, the menu bar menu shows a "Rain Sounds" on/off
toggle and a volume slider (and "Thunder" when separate clips exist).
Intensity scales the rain's loudness, Off is silent, and sound is muted
when the Mac sleeps, locks, switches away or a full-screen app covers it.

--- Licensing ------------------------------------------------------------

Only add audio you have the right to redistribute: everything in this
folder is copied into every build of the app. A recording under an
attribution-required or non-commercial licence would impose its terms on
the whole project, so CC0 / public-domain recordings are the safest choice.
Otherwise keep the file on your Mac and leave it out of the repository.
