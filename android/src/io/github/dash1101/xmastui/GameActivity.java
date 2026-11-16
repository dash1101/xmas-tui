package io.github.dash1101.xmastui;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.WallpaperManager;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Context;
import android.content.DialogInterface;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioTrack;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.FrameLayout;

/**
 * The village to play in: the desktop's own village (native), full screen,
 * with a joystick to walk about and buttons for what the keys do on the
 * desktop. Tap things as on the desktop; press and hold for what can be done
 * there; two fingers to look along the village.
 */
public class GameActivity extends Activity implements SurfaceHolder.Callback {
    private long game;
    private String cfgUsed = "";
    private SurfaceView surface;
    private Controls controls;
    private volatile boolean running;
    private Thread loop;
    private Thread sound;
    private final Handler ui = new Handler(Looper.getMainLooper());
    /** When this game last wrote the save (the wallpaper's later saves are picked up). */
    private long savedAt;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        requestWindowFeature(Window.FEATURE_NO_TITLE);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON | WindowManager.LayoutParams.FLAG_FULLSCREEN);
        if (Build.VERSION.SDK_INT >= 28) {
            getWindow().getAttributes().layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        }
        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(Color.rgb(3, 5, 15));
        surface = new SurfaceView(this);
        surface.getHolder().addCallback(this);
        root.addView(surface, new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));
        controls = new Controls(this);
        root.addView(controls, new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));
        setContentView(root);
        immersive();
        SharedPreferences p = Prefs.get(this);
        if (!p.getBoolean("game_seen", false)) {
            p.edit().putBoolean("game_seen", true).apply();
            ui.postDelayed(new Runnable() {
                @Override
                public void run() {
                    help();
                }
            }, 600);
        }
    }

    private void immersive() {
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController c = getWindow().getInsetsController();
            if (c != null) {
                c.hide(WindowInsets.Type.systemBars());
                c.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_FULLSCREEN
                    | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                    | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
        }
    }

    /** The settings that the village takes (shared with the wallpaper's), as TOML. */
    private String cfgToml() {
        return Prefs.toml(this, false);
    }

    /** The "Size" setting (bigger: bigger village, fewer rows on the screen). */
    private float size() {
        return Math.max(40, Math.min(250, Prefs.get(this).getInt("size", 100))) / 100f;
    }

    // ------------------------------------------------------------ surface

    @Override
    public void surfaceCreated(SurfaceHolder holder) {}

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int w, int h) {
        stopLoop();
        String cfg = cfgToml();
        if (game != 0 && !cfg.equals(cfgUsed)) {
            Native.destroy(game);
            game = 0;
        }
        // (a new size: a new village to fit it; the wallpaper's moved
        // things on since: from its save)
        java.io.File save = new java.io.File(getFilesDir(), "save.toml");
        if (game != 0 && (w != controls.gw || h != controls.gh || save.lastModified() > savedAt + 500)) {
            Native.destroy(game);
            game = 0;
        }
        if (game == 0) {
            game = Native.create(w, h, size(), cfg);
            savedAt = save.lastModified();
            cfgUsed = cfg;
            controls.gw = w;
            controls.gh = h;
        }
        Native.attach(game, holder.getSurface());
        startLoop();
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        stopLoop();
        if (game != 0) {
            Native.detach(game);
        }
    }

    private void startLoop() {
        if (game == 0 || running) {
            return;
        }
        running = true;
        final long g = game;
        final int fps = Math.max(15, Math.min(60, Prefs.get(this).getInt("game_fps", 30)));
        loop = new Thread(new Runnable() {
            @Override
            public void run() {
                long last = System.nanoTime();
                long step = 1_000_000_000L / fps;
                while (running) {
                    long now = System.nanoTime();
                    Native.frame(g, (now - last) / 1e9f);
                    last = now;
                    long wait = step - (System.nanoTime() - now);
                    if (wait > 0) {
                        try {
                            Thread.sleep(wait / 1_000_000L, (int) (wait % 1_000_000L));
                        } catch (InterruptedException e) {
                            return;
                        }
                    }
                }
            }
        }, "village");
        loop.start();
        startSound();
    }

    private void stopLoop() {
        running = false;
        joinQuietly(loop);
        joinQuietly(sound);
        loop = null;
        sound = null;
    }

    private static void joinQuietly(Thread t) {
        if (t == null) {
            return;
        }
        try {
            t.join(1000);
        } catch (InterruptedException ignored) {
        }
    }

    /** The village's sound, through an AudioTrack (it paces itself on the track). */
    private void startSound() {
        sound = new Thread(new Runnable() {
            @Override
            public void run() {
                int min = AudioTrack.getMinBufferSize(48000, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT);
                AudioTrack track;
                try {
                    track = new AudioTrack.Builder()
                            .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
                            .setAudioFormat(new AudioFormat.Builder().setSampleRate(48000).setEncoding(AudioFormat.ENCODING_PCM_16BIT).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build())
                            .setBufferSizeInBytes(Math.max(min, 48000 * 4 / 10))
                            .setTransferMode(AudioTrack.MODE_STREAM)
                            .build();
                } catch (Exception e) {
                    return;
                }
                track.play();
                short[] buf = new short[960 * 2];
                while (running) {
                    Native.audio(buf);
                    track.write(buf, 0, buf.length);
                }
                track.pause();
                track.flush();
                track.release();
            }
        }, "village-sound");
        sound.start();
    }

    // ------------------------------------------------------------ lifecycle

    @Override
    protected void onResume() {
        super.onResume();
        immersive();
        if (game != 0) {
            Native.pause(game, false);
        }
    }

    @Override
    protected void onPause() {
        super.onPause();
        if (game != 0) {
            Native.pause(game, true);
            savedAt = new java.io.File(getFilesDir(), "save.toml").lastModified();
        }
    }

    @Override
    protected void onDestroy() {
        stopLoop();
        if (game != 0) {
            Native.destroy(game);
            game = 0;
        }
        super.onDestroy();
    }

    @Override
    public void onWindowFocusChanged(boolean focus) {
        super.onWindowFocusChanged(focus);
        if (focus) {
            immersive();
        }
    }

    // ------------------------------------------------------------ menus

    /** What can be done at (x, y), as a list to pick from. */
    void longPress(float x, float y) {
        if (game == 0) {
            return;
        }
        showMenu(Native.menu(game, x, y));
    }

    /** Things to do right here (the ✦ button). */
    void todo() {
        if (game != 0) {
            Native.button(game, -1);
            showMenu(Native.todo(game));
        }
    }

    /** A list from the village ({title, choice, ...}); a choice with more behind it shows the next list. */
    private void showMenu(String[] m) {
        if (m == null || m.length < 2) {
            return;
        }
        String[] items = new String[m.length - 1];
        System.arraycopy(m, 1, items, 0, items.length);
        new AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                .setTitle(m[0])
                .setItems(items, new DialogInterface.OnClickListener() {
                    @Override
                    public void onClick(DialogInterface d, int which) {
                        final String[] next = Native.pick(game, which);
                        if (next != null) {
                            ui.post(new Runnable() {
                                @Override
                                public void run() {
                                    showMenu(next);
                                }
                            });
                        }
                    }
                })
                .show();
    }

    /** The other things the keys do: pockets, inviting someone along, what's in hand… */
    void more() {
        final String[] items = {"Pockets, scorecard & Christmas list", "Invite along / say goodbye", "What's in hand (berries, cocoa…)", "Lie down in the snow / get up", "Close what's open", "Set as the live wallpaper", "Settings", "Controls"};
        new AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                .setItems(items, new DialogInterface.OnClickListener() {
                    @Override
                    public void onClick(DialogInterface d, int which) {
                        switch (which) {
                            case 0:
                                Native.button(game, Native.POCKETS);
                                controls.pockets = !controls.pockets;
                                controls.invalidate();
                                break;
                            case 1:
                                Native.button(game, Native.INVITE);
                                break;
                            case 2:
                                Native.button(game, Native.HELD);
                                break;
                            case 3:
                                Native.button(game, Native.REST);
                                break;
                            case 4:
                                Native.button(game, Native.BACK);
                                controls.pockets = false;
                                controls.invalidate();
                                break;
                            case 5:
                                setWallpaper();
                                break;
                            case 6:
                                startActivity(new Intent(GameActivity.this, SettingsActivity.class));
                                break;
                            default:
                                help();
                        }
                    }
                })
                .show();
    }

    private void setWallpaper() {
        if (game != 0) {
            Native.save(game);
        }
        Intent i = new Intent(WallpaperManager.ACTION_CHANGE_LIVE_WALLPAPER);
        i.putExtra(WallpaperManager.EXTRA_LIVE_WALLPAPER_COMPONENT, new ComponentName(this, VillageWallpaper.class));
        try {
            startActivity(i);
        } catch (ActivityNotFoundException e) {
            try {
                startActivity(new Intent(WallpaperManager.ACTION_LIVE_WALLPAPER_CHOOSER));
            } catch (ActivityNotFoundException ignored) {
            }
        }
    }

    void help() {
        new AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                .setTitle("Playing in the village")
                .setMessage("Joystick (left): walk about; push it all the way to run.\n\n"
                        + "E: talk, use, pick up, help build (whatever's in front). In a conversation, push the joystick up or down to pick an answer, then E.\n"
                        + "Jump: jump; hold it to go higher (up to four times as high). Up against a house, a tree or a lamp post, it leaps right up onto it. Lying down: a snow angel.\n"
                        + "❄: scoop a snowball, then throw it\n"
                        + "✦: things to do here: build a snowman or an igloo, a snow angel, a rest in the snow, a carol, tag, a light show, fireworks, a snowball fight, a game at the table indoors, the train to the ski slopes, a drive\n"
                        + "☰: pockets & scorecard, invite someone along, lie down, settings, set as the wallpaper\n"
                        + "◎: the view back on the villager\n\n"
                        + "Tap people, animals and things as on the desktop. Press and hold anywhere for what can be done there. Drag people, animals and the pet to carry them. Two fingers look along the village. Back closes what's open, then leaves.\n\n"
                        + "A game controller: stick or d-pad to walk, A jump, X talk/use, Y snowball, B back, Start ☰, Select ✦, L1 ◎, R1 lie down.")
                .setPositiveButton("OK", null)
                .show();
    }

    @Override
    public void onBackPressed() {
        if (controls.pockets && game != 0) {
            Native.button(game, Native.BACK);
            controls.pockets = false;
            controls.invalidate();
            return;
        }
        super.onBackPressed();
    }

    // ------------------------------------------------------------ gamepad

    /** The d-pad's directions held (left, right, up, down). */
    private final boolean[] dpad = new boolean[4];

    @Override
    public boolean dispatchKeyEvent(KeyEvent e) {
        int src = e.getSource();
        boolean pad = (src & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD || (src & InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD || (src & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK;
        if (!pad || game == 0 || !hasWindowFocus()) {
            return super.dispatchKeyEvent(e);
        }
        boolean down = e.getAction() == KeyEvent.ACTION_DOWN;
        boolean first = down && e.getRepeatCount() == 0;
        controls.touched();
        switch (e.getKeyCode()) {
            case KeyEvent.KEYCODE_DPAD_LEFT: dpad[0] = down; padSteer(); return true;
            case KeyEvent.KEYCODE_DPAD_RIGHT: dpad[1] = down; padSteer(); return true;
            case KeyEvent.KEYCODE_DPAD_UP: dpad[2] = down; padSteer(); return true;
            case KeyEvent.KEYCODE_DPAD_DOWN: dpad[3] = down; padSteer(); return true;
            case KeyEvent.KEYCODE_BUTTON_A:
                if (first) Native.button(game, Native.JUMP);
                if (!down) Native.button(game, Native.JUMP_UP);
                return true;
            case KeyEvent.KEYCODE_BUTTON_X: if (first) Native.button(game, Native.E); return true;
            case KeyEvent.KEYCODE_BUTTON_Y: if (first) Native.button(game, Native.SNOWBALL); return true;
            case KeyEvent.KEYCODE_BUTTON_B: if (first) onBackPressed(); return true;
            case KeyEvent.KEYCODE_BUTTON_START: if (first) more(); return true;
            case KeyEvent.KEYCODE_BUTTON_SELECT: if (first) todo(); return true;
            case KeyEvent.KEYCODE_BUTTON_L1: if (first) Native.button(game, Native.CENTRE); return true;
            case KeyEvent.KEYCODE_BUTTON_R1: if (first) Native.button(game, Native.REST); return true;
            default: return super.dispatchKeyEvent(e);
        }
    }

    private float stickX, stickY;

    private void padSteer() {
        float mx = (dpad[1] ? 1 : 0) - (dpad[0] ? 1 : 0);
        float my = (dpad[3] ? 1 : 0) - (dpad[2] ? 1 : 0);
        if (mx == 0 && my == 0) {
            mx = Math.abs(stickX) < 0.35f ? 0 : Math.signum(stickX);
            my = Math.abs(stickY) < 0.45f ? 0 : Math.signum(stickY);
        }
        boolean run = stickX * stickX + stickY * stickY > 0.9f;
        Native.steer(game, mx, my, run);
    }

    @Override
    public boolean onGenericMotionEvent(MotionEvent e) {
        if ((e.getSource() & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK && e.getAction() == MotionEvent.ACTION_MOVE && game != 0) {
            float hx = e.getAxisValue(MotionEvent.AXIS_HAT_X), hy = e.getAxisValue(MotionEvent.AXIS_HAT_Y);
            stickX = Math.abs(hx) > 0.5f ? hx : e.getAxisValue(MotionEvent.AXIS_X);
            stickY = Math.abs(hy) > 0.5f ? hy : e.getAxisValue(MotionEvent.AXIS_Y);
            controls.touched();
            padSteer();
            return true;
        }
        return super.onGenericMotionEvent(e);
    }

    // ------------------------------------------------------------ controls

    /**
     * The joystick and buttons drawn over the village, in chunky pixel art
     * like the village itself; touches anywhere else go to the village
     * (taps, drags, long presses, two-finger looks). With "hide the
     * controls" on, they fade away when left alone, and come back at a touch.
     */
    final class Controls extends View {
        int gw, gh;
        boolean pockets;
        private final Paint fill = new Paint();
        private final float d = getResources().getDisplayMetrics().density;
        /** One art pixel, in screen pixels. */
        private final int P = Math.max(2, Math.round(3 * getResources().getDisplayMetrics().density));
        /** The joystick's pointer, its centre and the thumb. */
        private int stickId = -1;
        private float tx, ty;
        /** The finger on the village (not a control): id, where it went down, when. */
        private int villageId = -1;
        private float vx, vy;
        private boolean panning, held;
        private float panX;
        private long lastTouch = System.currentTimeMillis();
        private final int slop = ViewConfiguration.get(GameActivity.this).getScaledTouchSlop();
        private final Runnable longPress = new Runnable() {
            @Override
            public void run() {
                if (villageId >= 0 && !panning) {
                    held = true;
                    Native.touch(game, 3, vx, vy);
                    GameActivity.this.longPress(vx, vy);
                }
            }
        };
        private final Runnable fade = new Runnable() {
            @Override
            public void run() {
                invalidate();
                if (alpha() > 0f) {
                    postDelayed(this, 80);
                }
            }
        };
        private final int[] pressed = new int[8];

        Controls(Context c) {
            super(c);
            fill.setAntiAlias(false);
            for (int i = 0; i < pressed.length; i++) {
                pressed[i] = -1;
            }
        }

        /** Someone's using the controls (a touch, a game controller): they show. */
        void touched() {
            lastTouch = System.currentTimeMillis();
            removeCallbacks(fade);
            postDelayed(fade, 80);
            invalidate();
        }

        /** How visible they are (1 shown .. 0 faded away). */
        private float alpha() {
            if (!Prefs.get(GameActivity.this).getBoolean("hide_controls", false)) {
                return 1f;
            }
            float idle = (System.currentTimeMillis() - lastTouch) / 1000f;
            return Math.max(0f, Math.min(1f, 1f - (idle - 6f)));
        }

        private float stickR() {
            return 58 * d;
        }

        private float stickX() {
            return 40 * d + stickR();
        }

        private float stickY() {
            return getHeight() - 36 * d - stickR();
        }

        /** The buttons: {x, y, radius} for E, jump, snowball, more, settings, centre, things to do. */
        private float[] btn(int i) {
            float w = getWidth(), h = getHeight();
            switch (i) {
                case 0: return new float[] {w - 70 * d, h - 82 * d, 40 * d};
                case 1: return new float[] {w - 170 * d, h - 52 * d, 32 * d};
                case 2: return new float[] {w - 62 * d, h - 182 * d, 28 * d};
                case 3: return new float[] {w - 150 * d, h - 150 * d, 26 * d};
                case 4: return new float[] {w - 30 * d, 30 * d, 20 * d};
                case 5: return new float[] {w - 80 * d, 30 * d, 20 * d};
                default: return new float[] {w - 230 * d, h - 120 * d, 26 * d};
            }
        }

        private static final int N_BTN = 7;

        /** 7×7 pixel icons for the buttons ('#' lit, '+' bright). */
        private final String[][] icons = {
            {"#####..", "##.....", "##.....", "####...", "##.....", "##.....", "#####.."},
            {"...#...", "..###..", ".#####.", "#######", "..###..", "..###..", "..###.."},
            {"..###..", ".##+##.", "##+++##", "#######", "#######", ".#####.", "..###.."},
            {".......", "#######", ".......", "#######", ".......", "#######", "......."},
            {"..#.#..", ".#####.", "##...##", ".#...#.", "##...##", ".#####.", "..#.#.."},
            {"..###..", ".#...#.", "#..#..#", "#.###.#", "#..#..#", ".#...#.", "..###.."},
            {"...#...", "...#...", ".#####.", "#######", ".#####.", "...#...", "...#..."},
        };

        /** A filled disc of art pixels, edged, round (cx, cy) radius r (screen px). */
        private void disc(Canvas c, float cx, float cy, float r, int body, int edge, int shade) {
            int n = (int) Math.ceil(r / P);
            int ox = Math.round(cx / P) * P, oy = Math.round(cy / P) * P;
            for (int j = -n; j <= n; j++) {
                for (int i = -n; i <= n; i++) {
                    float dist = (float) Math.sqrt(i * i + j * j) * P;
                    if (dist > r) {
                        continue;
                    }
                    int col = dist > r - P ? edge : (j > n * 0.45f ? shade : body);
                    fill.setColor(col);
                    c.drawRect(ox + i * P, oy + j * P, ox + (i + 1) * P, oy + (j + 1) * P, fill);
                }
            }
        }

        /** A ring of art pixels. */
        private void ring(Canvas c, float cx, float cy, float r, int col) {
            int n = (int) Math.ceil(r / P);
            int ox = Math.round(cx / P) * P, oy = Math.round(cy / P) * P;
            fill.setColor(col);
            for (int j = -n; j <= n; j++) {
                for (int i = -n; i <= n; i++) {
                    float dist = (float) Math.sqrt(i * i + j * j) * P;
                    if (dist <= r && dist > r - P) {
                        c.drawRect(ox + i * P, oy + j * P, ox + (i + 1) * P, oy + (j + 1) * P, fill);
                    }
                }
            }
        }

        private void icon(Canvas c, String[] art, float cx, float cy, int col, int bright, int k) {
            int s = P * k;
            int ox = Math.round(cx / P) * P - s * 7 / 2, oy = Math.round(cy / P) * P - s * 7 / 2;
            for (int j = 0; j < art.length; j++) {
                for (int i = 0; i < art[j].length(); i++) {
                    char ch = art[j].charAt(i);
                    if (ch == '.') {
                        continue;
                    }
                    fill.setColor(ch == '+' ? bright : col);
                    c.drawRect(ox + i * s, oy + j * s, ox + (i + 1) * s, oy + (j + 1) * s, fill);
                }
            }
        }

        private int a(int alpha, int r, int g, int b, float k) {
            return Color.argb(Math.round(alpha * k), r, g, b);
        }

        @Override
        protected void onDraw(Canvas c) {
            float k = alpha();
            if (k <= 0f) {
                return;
            }
            // the joystick: a frosty ring, the thumb a pale disc
            disc(c, stickX(), stickY(), stickR(), a(60, 200, 220, 255, k), a(170, 255, 211, 107, k), a(60, 150, 170, 220, k));
            ring(c, stickX(), stickY(), stickR() * 0.62f, a(70, 255, 255, 255, k));
            float kx = stickId >= 0 ? tx : stickX(), ky = stickId >= 0 ? ty : stickY();
            disc(c, kx, ky, stickR() * 0.42f, a(stickId >= 0 ? 220 : 160, 255, 236, 200, k), a(220, 255, 211, 107, k), a(200, 220, 190, 150, k));
            // the buttons: pixel discs, a gold rim, a shade underneath; pressed, they sink
            for (int i = 0; i < N_BTN; i++) {
                float[] b = btn(i);
                boolean down = false;
                for (int p : pressed) {
                    down |= p == i;
                }
                float sink = down ? P : 0;
                int body = i == 0 ? a(down ? 230 : 190, 200, 50, 60, k) : (i == 6 ? a(down ? 220 : 170, 40, 90, 70, k) : a(down ? 220 : 160, 30, 40, 80, k));
                int shade = i == 0 ? a(210, 140, 30, 40, k) : a(190, 18, 24, 50, k);
                disc(c, b[0], b[1] + sink, b[2], body, a(220, 255, 211, 107, k), shade);
                int icol = i == 6 ? a(255, 255, 220, 120, k) : a(255, 250, 246, 236, k);
                icon(c, icons[i], b[0], b[1] + sink - P, icol, a(255, 255, 255, 255, k), b[2] > 30 * d ? 2 : 1);
            }
            if (pockets) {
                for (int n = 1; n <= 5; n++) {
                    float[] b = num(n);
                    disc(c, b[0], b[1], b[2], a(170, 30, 40, 80, k), a(220, 255, 211, 107, k), a(190, 18, 24, 50, k));
                    icon(c, digit(n), b[0], b[1], a(255, 255, 255, 255, k), a(255, 255, 255, 255, k), 1);
                }
            }
        }

        private String[] digit(int n) {
            switch (n) {
                case 1: return new String[] {"...#...", "..##...", "...#...", "...#...", "...#...", "...#...", "..###.."};
                case 2: return new String[] {"..###..", ".#...#.", ".....#.", "....#..", "...#...", "..#....", ".#####."};
                case 3: return new String[] {".####..", ".....#.", ".....#.", "..###..", ".....#.", ".....#.", ".####.."};
                case 4: return new String[] {"....#..", "...##..", "..#.#..", ".#..#..", ".#####.", "....#..", "....#.."};
                default: return new String[] {".#####.", ".#.....", ".####..", ".....#.", ".....#.", ".#...#.", "..###.."};
            }
        }

        /** The number row (pockets): {x, y, r} for 1..5. */
        private float[] num(int n) {
            float w = getWidth();
            return new float[] {w / 2f + (n - 3) * 54 * d, 34 * d, 22 * d};
        }

        private int hitButton(float x, float y) {
            for (int i = 0; i < N_BTN; i++) {
                float[] b = btn(i);
                float dx = x - b[0], dy = y - b[1];
                if (dx * dx + dy * dy <= (b[2] + 10 * d) * (b[2] + 10 * d)) {
                    return i;
                }
            }
            if (pockets) {
                for (int n = 1; n <= 5; n++) {
                    float[] b = num(n);
                    float dx = x - b[0], dy = y - b[1];
                    if (dx * dx + dy * dy <= (b[2] + 8 * d) * (b[2] + 8 * d)) {
                        return 100 + n;
                    }
                }
            }
            return -1;
        }

        private void press(int b) {
            if (game == 0) {
                return;
            }
            switch (b) {
                case 0: Native.button(game, Native.E); break;
                case 1: Native.button(game, Native.JUMP); break;
                case 2: Native.button(game, Native.SNOWBALL); break;
                case 3: more(); break;
                case 4: startActivity(new Intent(GameActivity.this, SettingsActivity.class)); break;
                case 5: Native.button(game, Native.CENTRE); break;
                case 6: todo(); break;
                default:
                    if (b > 100) {
                        Native.button(game, Native.NUM + (b - 100));
                    }
            }
        }

        private void steer() {
            float r = stickR();
            float dx = (tx - stickX()) / r, dy = (ty - stickY()) / r;
            float len = (float) Math.sqrt(dx * dx + dy * dy);
            if (len > 1f) {
                dx /= len;
                dy /= len;
            }
            float mx = Math.abs(dx) < 0.3f ? 0 : Math.signum(dx);
            float my = Math.abs(dy) < 0.4f ? 0 : Math.signum(dy);
            Native.steer(game, mx, my, len > 0.95f);
        }

        @Override
        public boolean onTouchEvent(MotionEvent e) {
            if (game == 0) {
                return true;
            }
            touched();
            int act = e.getActionMasked();
            int idx = e.getActionIndex();
            int id = e.getPointerId(idx);
            float x = e.getX(idx), y = e.getY(idx);
            switch (act) {
                case MotionEvent.ACTION_DOWN:
                case MotionEvent.ACTION_POINTER_DOWN: {
                    float dx = x - stickX(), dy = y - stickY();
                    if (stickId < 0 && dx * dx + dy * dy <= stickR() * stickR() * 2.2f) {
                        stickId = id;
                        tx = x;
                        ty = y;
                        steer();
                        invalidate();
                        break;
                    }
                    int b = hitButton(x, y);
                    if (b >= 0) {
                        if (id < pressed.length) {
                            pressed[id] = b;
                        }
                        press(b);
                        invalidate();
                        break;
                    }
                    if (villageId < 0) {
                        villageId = id;
                        vx = x;
                        vy = y;
                        held = false;
                        panning = false;
                        Native.touch(game, 0, x, y);
                        postDelayed(longPress, ViewConfiguration.getLongPressTimeout());
                    } else if (!panning) {
                        // a second finger: looking along the village
                        removeCallbacks(longPress);
                        Native.touch(game, 3, vx, vy);
                        panning = true;
                        panX = (vx + x) / 2f;
                    }
                    break;
                }
                case MotionEvent.ACTION_MOVE: {
                    for (int i = 0; i < e.getPointerCount(); i++) {
                        int pid = e.getPointerId(i);
                        float px = e.getX(i), py = e.getY(i);
                        if (pid == stickId) {
                            tx = px;
                            ty = py;
                            steer();
                            invalidate();
                        } else if (pid == villageId && !held) {
                            if (panning) {
                                float mid = e.getPointerCount() > 1 ? (e.getX(0) + e.getX(1)) / 2f : px;
                                Native.pan(game, mid - panX);
                                panX = mid;
                            } else {
                                if (Math.abs(px - vx) > slop || Math.abs(py - vy) > slop) {
                                    removeCallbacks(longPress);
                                }
                                Native.touch(game, 1, px, py);
                            }
                        }
                    }
                    break;
                }
                case MotionEvent.ACTION_UP:
                case MotionEvent.ACTION_POINTER_UP:
                case MotionEvent.ACTION_CANCEL: {
                    boolean cancel = act == MotionEvent.ACTION_CANCEL;
                    if (id == stickId || (cancel && stickId >= 0)) {
                        stickId = -1;
                        Native.steer(game, 0, 0, false);
                        invalidate();
                    }
                    if (id < pressed.length && pressed[id] >= 0) {
                        // the jump button let go of: a held jump stops rising
                        if (pressed[id] == 1) {
                            Native.button(game, Native.JUMP_UP);
                        }
                        pressed[id] = -1;
                        invalidate();
                    }
                    if (id == villageId || (cancel && villageId >= 0)) {
                        removeCallbacks(longPress);
                        if (!held && !panning) {
                            Native.touch(game, cancel ? 3 : 2, x, y);
                        }
                        villageId = -1;
                        panning = false;
                        held = false;
                    }
                    break;
                }
                default:
                    break;
            }
            return true;
        }
    }
}
