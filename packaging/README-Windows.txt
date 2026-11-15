xmas-tui — a cozy, glowing Christmas scene as your live desktop wallpaper
=========================================================================

START IT
  Double-click xmas-tui.exe. It tucks itself behind your desktop icons on
  your main monitor and becomes the wallpaper. Nothing gets installed.

  Windows will probably warn "Windows protected your PC", because the
  program isn't signed. Click "More info", then "Run anyway".

SETTINGS
  Click the ⚙ in the top-right corner of your desktop. From there you can:
    - adjust snow, wind, glow, how often surprises happen, Santa's sleigh,
      the clock and the widgets
    - turn on "Start with Windows" so it runs every time you log in
    - "✕ quit wallpaper" to stop it

  Weather city: clicking the city field opens the settings file in Notepad.
  Change   city = "..."   to your city, save, and the wallpaper picks it
  up within a couple of seconds.

PLAY WITH IT
  - Drag any widget to move it.
  - Your cursor leaves a sparkly trail and pushes the falling snow around.
    Sweep through the snowy ground to kick snow up.
  - Click things on the bare desktop:
      presents, trees, the star on top, the snowman (try a few times),
      houses, the church, lamps, the moon, the sky, the snow.
  - Random things happen on their own: deer, foxes, people with lanterns,
    a train, meteor showers, light shows, bells on the hour…

  It pauses automatically while a fullscreen game is running.

WALK AROUND THE VILLAGE
  Click the bare desktop (so it has keyboard focus), then:
    WASD / arrow keys   walk your little villager; the view follows you
    E, Space or Enter   say hello, pet an animal, pick berries, poke things
    F                   ask someone to walk with you (animals want a berry);
                        F with nobody in front of you says goodbye to all
  Stand still near someone with your companions and they start chatting.

UPDATES
  When a new version is out, a small banner at the top asks whether to
  install it. To have it install updates on its own, open settings,
  Display tab, and tick "Install automatically".

IF IT SHOWS UP AS A NORMAL WINDOW INSTEAD OF THE WALLPAPER
  Some Windows versions change how the desktop works. In that case, use the
  free Lively Wallpaper app (Microsoft Store): add xmas-tui.exe as an
  "Application" wallpaper with the argument   --windowed

FILES
  Settings: %APPDATA%\xmas-tui\config.toml
  To remove it: open settings, untick "Start with Windows", click
  "✕ quit wallpaper", then delete xmas-tui.exe and the %APPDATA%\xmas-tui folder.

Fonts: DejaVu (Bitstream Vera license) and Noto Sans Symbols 2 (SIL OFL),
bundled inside the program; see the LICENSE-*.txt files.
