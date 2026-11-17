package io.github.dash1101.xmastui;

import android.view.Surface;

/**
 * The village itself (the desktop's own, built as libxmas.so from
 * android/native): what the game screen calls. A game is a handle; frames
 * are drawn straight onto the surface it's attached to.
 */
final class Native {
    static {
        System.loadLibrary("xmas");
    }

    private Native() {}

    /** Buttons (and what they are on the desktop). */
    static final int E = 0, JUMP = 1, SNOWBALL = 2, POCKETS = 3, INVITE = 4, HELD = 5, BACK = 6, CENTRE = 7, JUMP_UP = 8, REST = 9, NUM = 10;

    /** A village for a w x h screen; size scales it (1 = about 18 px a row); cfg is TOML. */
    static native long create(int w, int h, float size, String cfg);

    /** The menu (its home page and the settings), open or shut. */
    static native void openPanel(long game);

    static native void destroy(long game);

    static native void attach(long game, Surface surface);

    static native void detach(long game);

    /** The village moves on dt seconds and is drawn onto the surface. */
    static native void frame(long game, float dt);

    /** The joystick: -1..1 each way; run when it's pushed all the way. */
    static native void steer(long game, float mx, float my, boolean run);

    static native void button(long game, int b);

    /** A finger on the village: 0 down, 1 move, 2 up, 3 cancel. */
    static native void touch(long game, int action, float x, float y);

    /** Two fingers along the village (pixels). */
    static native void pan(long game, float dx);

    /** A long press: {title, choice, ...}, or null. */
    static native String[] menu(long game, float x, float y);

    /** A choice from the menu shown: null, or a further menu {title, choice, ...}. */
    static native String[] pick(long game, int i);

    /** Things to do right here: {title, choice, ...}, or null. */
    static native String[] todo(long game);

    /** The ☰ menu, drawn in the village. */
    static native void more(long game);

    /** Friends & online (the ✉ button). */
    static native void online(long game);

    /** The controls card, drawn in the village. */
    static native void help(long game);

    /** Back: whether it closed something (a menu, a card, the pockets…). */
    static native boolean back(long game);

    /** What's showing: 1 the pockets, 2 a menu or card. */
    static native int state(long game);

    /** Something for the app to do ("wallpaper", "settings", "text:<question>", "copy:<text>",
     *  "you:<json>" the villager changed, "reload" another villager), or null. */
    static native String ask(long game);

    /** The line typed for a "text:" ask ("" for never mind). */
    static native void giveText(long game, String text);

    /** A save's seed (its text), or 0 when it isn't one. */
    static native long saveSeed(String text);

    /** The villager in a save (its text) as JSON: name, pet, friends, look; or null. */
    static native String youJson(String text);

    /** Let go of the save file (another's been put in its place): nothing more is written to it. */
    static native void forget(long game);

    /** Write the save file now (shared by the game and the wallpaper). */
    static native void save(long game);

    static native void pause(long game, boolean on);

    /** New settings (the config text), taken in where the game stands. */
    static native void configure(long game, String cfg, float size);

    /** The screen's new size: the grid made again, the game going on. */
    static native void resize(long game, int w, int h, float size);

    /** The talk line. */
    static native void chat(long game);

    /** Sound for the AudioTrack (48 kHz stereo); how many samples were real. */
    static native int audio(short[] buf);
}
