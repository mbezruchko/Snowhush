Snowhush rain sounds
====================

Drop audio files in this folder, then run ./build.sh (or build in Xcode).
build.sh copies this folder into Snowhush.app/Contents/Resources/Sounds.
Accepted extensions: .wav .aiff .aif .m4a .mp3 .caf .aac
Non-audio files (like this one) are ignored. The app reads this folder when
it launches, so restart Snowhush after adding or removing sounds.

--- The simple setup: one combined file ---------------------------------

Put a single rain-and-thunder file in here, under any name, for example:

  rain-and-thunder.mp3

It is looped forever while Rain mode is on, and the volume slider, the
Rain Sounds toggle, Intensity and the sleep/lock muting all apply to it.
The thunder baked into the file comes round again on each loop, and the
"Thunder" menu item stays hidden because there is nothing separate to
switch off.

For a perfectly seamless join, WAV, AIFF or CAF is best: MP3 often carries
a fraction of a second of encoder padding at the start and end, which can
be audible at the loop point. If you hear a seam, convert the file.

--- The fuller setup: a rain bed plus separate thunder ------------------

  rain.mp3             A rain-only bed, looped forever.
  thunder-1.mp3        One-shot claps. Any number may be added; one is
  thunder-2.mp3        picked at random every 20-75 seconds (more often
  thunder-3.mp3        in heavy rain, less in a light shower). A few at
                       different distances keep repeats from standing out.
                       Anything whose name starts with "thunder" counts.

With separate thunder files present, the menu bar menu gains a "Thunder"
on/off toggle alongside "Rain Sounds" and the volume slider. Otherwise the
single combined file above is used as the loop.

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
