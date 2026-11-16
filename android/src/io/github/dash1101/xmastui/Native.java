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
    static final int E = 0, JUMP = 1, SNOWBALL = 2, POCKETS = 3, INVITE = 4, HELD = 5, BACK = 6, NUM = 10;

    /** A village for a w x h screen; size scales it (1 = about 18 px a row); cfg is TOML. */
    static native long create(int w, int h, float size, String cfg);

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

    static native void pick(long game, int i);

    static native void pause(long game, boolean on);

    /** Sound for the AudioTrack (48 kHz stereo); how many samples were real. */
    static native int audio(short[] buf);
}
