package io.github.dash1101.xmastui;

import android.app.Activity;
import android.app.AlertDialog;
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
        SharedPreferences p = Prefs.get(this);
        String time = p.getString("time", "auto");
        if (time.equals("day")) {
            time = "noon";
        }
        StringBuilder s = new StringBuilder();
        s.append("[scene]\n");
        s.append("name = ").append(q(p.getString("name", ""))).append('\n');
        s.append("time = ").append(q(time)).append('\n');
        s.append("snow = ").append(p.getInt("snow", 100) / 100f).append('\n');
        s.append("[scene.pet]\n");
        s.append("kind = ").append(q(p.getString("pet", "cat"))).append('\n');
        s.append("name = ").append(q(p.getString("pet_name", ""))).append('\n');
        s.append("[sound]\n");
        s.append("enabled = ").append(p.getBoolean("game_sound", true)).append('\n');
        return s.toString();
    }

    private static String q(String v) {
        return "\"" + v.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
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
        // (a new size: a new village to fit it)
        if (game != 0 && (w != controls.gw || h != controls.gh)) {
            Native.destroy(game);
            game = 0;
        }
        if (game == 0) {
            game = Native.create(w, h, size(), cfg);
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
        String[] m = Native.menu(game, x, y);
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
                        Native.pick(game, which);
                    }
                })
                .show();
    }

    /** The other things the keys do: pockets, inviting someone along, what's in hand… */
    void more() {
        final String[] items = {"Pockets & Christmas list", "Invite along / say goodbye", "What's in hand (berries, cocoa…)", "Close what's open", "Settings", "Controls"};
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
                                Native.button(game, Native.BACK);
                                controls.pockets = false;
                                controls.invalidate();
                                break;
                            case 4:
                                startActivity(new Intent(GameActivity.this, SettingsActivity.class));
                                break;
                            default:
                                help();
                        }
                    }
                })
                .show();
    }

    void help() {
        new AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                .setTitle("Playing in the village")
                .setMessage("Joystick (left): walk about; push it all the way to run. Up against a house, a tree or a lamp post, push up to climb it.\n\n"
                        + "E: talk, use, pick up, help build (whatever's in front)\n"
                        + "Jump: jump (steer in the air with the joystick)\n"
                        + "❄: scoop a snowball, then throw it\n"
                        + "☰: pockets, invite someone along, what's in hand, settings\n\n"
                        + "Tap people, animals and things as on the desktop. Press and hold anywhere for what can be done there. Drag people and animals to carry them. Two fingers look along the village. Back closes what's open, then leaves.")
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

    // ------------------------------------------------------------ controls

    /**
     * The joystick and buttons drawn over the village; touches anywhere
     * else go to the village (taps, drags, long presses, two-finger looks).
     */
    final class Controls extends View {
        int gw, gh;
        boolean pockets;
        private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint ring = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint text = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final float d = getResources().getDisplayMetrics().density;
        /** The joystick's pointer, its centre and the thumb. */
        private int stickId = -1;
        private float sx, sy, tx, ty;
        /** The finger on the village (not a control): id, where it went down, when. */
        private int villageId = -1;
        private float vx, vy;
        private boolean panning, held;
        private float panX;
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
        private final int[] pressed = new int[8];

        Controls(Context c) {
            super(c);
            ring.setStyle(Paint.Style.STROKE);
            ring.setStrokeWidth(2 * d);
            text.setTextAlign(Paint.Align.CENTER);
            text.setColor(Color.WHITE);
            text.setFakeBoldText(true);
            for (int i = 0; i < pressed.length; i++) {
                pressed[i] = -1;
            }
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

        /** The buttons: {x, y, radius} for E, jump, snowball, more, settings. */
        private float[] btn(int i) {
            float w = getWidth(), h = getHeight();
            switch (i) {
                case 0: return new float[] {w - 70 * d, h - 82 * d, 40 * d};
                case 1: return new float[] {w - 170 * d, h - 52 * d, 32 * d};
                case 2: return new float[] {w - 62 * d, h - 182 * d, 28 * d};
                case 3: return new float[] {w - 150 * d, h - 150 * d, 26 * d};
                default: return new float[] {w - 30 * d, 30 * d, 20 * d};
            }
        }

        private static final int N_BTN = 5;
        private final String[] labels = {"E", "jump", "❄", "☰", "⚙"};

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
                default:
                    if (b > 100) {
                        Native.button(game, Native.NUM + (b - 100));
                    }
            }
        }

        @Override
        protected void onDraw(Canvas c) {
            // the joystick
            fill.setColor(Color.argb(46, 255, 255, 255));
            c.drawCircle(stickX(), stickY(), stickR(), fill);
            ring.setColor(Color.argb(110, 255, 255, 255));
            c.drawCircle(stickX(), stickY(), stickR(), ring);
            float kx = stickId >= 0 ? tx : stickX(), ky = stickId >= 0 ? ty : stickY();
            fill.setColor(Color.argb(stickId >= 0 ? 170 : 110, 255, 236, 200));
            c.drawCircle(kx, ky, stickR() * 0.42f, fill);
            // the buttons
            for (int i = 0; i < N_BTN; i++) {
                float[] b = btn(i);
                boolean down = false;
                for (int p : pressed) {
                    down |= p == i;
                }
                fill.setColor(i == 0 ? Color.argb(down ? 200 : 140, 214, 60, 70) : Color.argb(down ? 170 : 90, 30, 40, 70));
                c.drawCircle(b[0], b[1], b[2], fill);
                ring.setColor(Color.argb(150, 255, 211, 107));
                c.drawCircle(b[0], b[1], b[2], ring);
                text.setTextSize((labels[i].length() > 1 ? 13 : 20) * d);
                c.drawText(labels[i], b[0], b[1] + text.getTextSize() * 0.35f, text);
            }
            if (pockets) {
                for (int n = 1; n <= 5; n++) {
                    float[] b = num(n);
                    fill.setColor(Color.argb(150, 30, 40, 70));
                    c.drawCircle(b[0], b[1], b[2], fill);
                    ring.setColor(Color.argb(150, 255, 211, 107));
                    c.drawCircle(b[0], b[1], b[2], ring);
                    text.setTextSize(16 * d);
                    c.drawText(String.valueOf(n), b[0], b[1] + 6 * d, text);
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
            int a = e.getActionMasked();
            int idx = e.getActionIndex();
            int id = e.getPointerId(idx);
            float x = e.getX(idx), y = e.getY(idx);
            switch (a) {
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
                    boolean cancel = a == MotionEvent.ACTION_CANCEL;
                    if (id == stickId || (cancel && stickId >= 0)) {
                        stickId = -1;
                        Native.steer(game, 0, 0, false);
                        invalidate();
                    }
                    if (id < pressed.length && pressed[id] >= 0) {
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
