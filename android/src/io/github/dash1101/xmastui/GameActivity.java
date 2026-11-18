package io.github.dash1101.xmastui;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.app.WallpaperManager;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Context;
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
    /** Everything on the screen (the village, the controls, the settings over them). */
    private FrameLayout root;
    /** The settings, over the village while they're open (null: shut). */
    private SettingsPanel settings;
    /**
     * A line typed in the village itself (the chat box, a friend's chat, a
     * code): typed here, in a box out of sight that brings the keyboard up,
     * and drawn in the village, above the keyboard.
     */
    private KbField kb;
    private boolean kbOn;
    /** (Set from here, not typed: not sent back.) */
    private boolean kbSetting;
    private volatile boolean running;
    private Thread loop;
    private Thread sound;
    private final Handler ui = new Handler(Looper.getMainLooper());
    /** The frame rate the loop runs at. */
    private int loopFps;
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
        root = new FrameLayout(this);
        root.setBackgroundColor(Color.rgb(3, 5, 15));
        surface = new SurfaceView(this);
        surface.getHolder().addCallback(this);
        root.addView(surface, new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));
        controls = new Controls(this);
        root.addView(controls, new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));
        kb = new KbField(this);
        root.addView(kb, new FrameLayout.LayoutParams(2, 2, android.view.Gravity.BOTTOM | android.view.Gravity.START));
        setContentView(root);
        // the keyboard over the village, not pushing it about: what's typed
        // is drawn above it (told how tall it is); older phones pan up to
        // the box at the bottom instead
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_NOTHING);
            root.setOnApplyWindowInsetsListener((v, in) -> {
                int h = in.getInsets(WindowInsets.Type.ime()).bottom;
                boolean shown = in.isVisible(WindowInsets.Type.ime());
                if (game != 0) Native.setKeyboard(game, shown ? h : 0);
                // (put away while a line's open: the line's closed)
                if (!shown && kbOn && kb.wasShown) {
                    kb.wasShown = false;
                    kbOn = false;
                    if (game != 0) Native.typeKey(game, 2);
                }
                if (shown) kb.wasShown = true;
                return in;
            });
        } else {
            getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_PAN);
        }
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
        // the menus' and talk's letters about 13 points tall (the screen's
        // own density); the menu's sizes go on top
        return Math.max(0.5f, Math.min(4f, getResources().getDisplayMetrics().density * 0.62f));
    }

    // ------------------------------------------------------------ surface

    @Override
    public void surfaceCreated(SurfaceHolder holder) {}

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int w, int h) {
        stopLoop();
        String cfg = cfgToml();
        java.io.File save = new java.io.File(getFilesDir(), "save.toml");
        // (a save imported, or a new village asked for, in the settings:
        // this one goes without writing over it, and the new one starts)
        boolean replaced = save.lastModified() > savedAt + 500 || (!save.exists() && savedAt > 0);
        if (game != 0 && replaced) {
            Native.forget(game);
            Native.destroy(game);
            game = 0;
        }
        // new settings, a new size (the phone turned): taken in by the game
        // where it stands (still in a friend's village, still online)
        if (game != 0 && !cfg.equals(cfgUsed)) {
            Native.configure(game, cfg, size());
            cfgUsed = cfg;
        }
        if (game != 0 && (w != controls.gw || h != controls.gh)) {
            Native.resize(game, w, h, size());
            controls.gw = w;
            controls.gh = h;
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
        loopFps = fps;
        loop = new Thread(new Runnable() {
            @Override
            public void run() {
                long last = System.nanoTime();
                long step = 1_000_000_000L / fps;
                while (running) {
                    long now = System.nanoTime();
                    Native.frame(g, (now - last) / 1e9f);
                    poll(g);
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
        if (controls != null) {
            controls.rescale();
        }
        if (game != 0) {
            Native.pause(game, false);
            // back from the settings: what changed, taken in
            String cfg = cfgToml();
            if (!cfg.equals(cfgUsed)) {
                Native.configure(game, cfg, size());
                cfgUsed = cfg;
            }
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

    /** What can be done at (x, y): a menu in the village. */
    void longPress(float x, float y) {
        if (game != 0) {
            Native.menu(game, x, y);
        }
    }

    /** Things to do right here (the ✦ button), in the village. */
    void todo() {
        if (game != 0) {
            Native.button(game, -1);
            Native.todo(game);
        }
    }

    /** The ☰ menu, in the village. */
    void more() {
        if (game != 0) {
            Native.more(game);
        }
    }

    /** A line for the village to have (a friend's code, a message): typed, then handed over. */
    static String typingCode(String v, boolean more) {
        int sp = more ? v.indexOf(' ') : -1;
        String code = sp >= 0 ? v.substring(0, sp) : v, rest = sp >= 0 ? v.substring(sp) : "";
        StringBuilder n = new StringBuilder();
        for (char c : code.toCharArray()) {
            if (n.length() < 14 && Character.isLetterOrDigit(c) && c < 128) {
                if (n.length() == 4 || n.length() == 9) n.append('-');
                n.append(Character.toUpperCase(c));
            }
        }
        return n + rest;
    }

    /** The box out of sight that the keyboard types into (see `kb`). */
    final class KbField extends android.widget.EditText {
        boolean wasShown;

        KbField(Context c) {
            super(c);
            setAlpha(0f);
            setSingleLine(true);
            setBackground(null);
            setCursorVisible(false);
            setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES);
            // (in landscape, no keyboard over the whole screen: the village
            // shows above it, with the line in it)
            setImeOptions(android.view.inputmethod.EditorInfo.IME_ACTION_SEND | android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI | android.view.inputmethod.EditorInfo.IME_FLAG_NO_FULLSCREEN);
            setFilters(new android.text.InputFilter[] {new android.text.InputFilter.LengthFilter(280)});
            addTextChangedListener(new android.text.TextWatcher() {
                @Override
                public void beforeTextChanged(CharSequence s, int a, int b, int c) {}

                @Override
                public void onTextChanged(CharSequence s, int a, int b, int c) {}

                @Override
                public void afterTextChanged(android.text.Editable s) {
                    if (!kbSetting && kbOn && game != 0) Native.typed(game, s.toString(), s.toString().codePointCount(0, Math.max(0, Math.min(s.length(), getSelectionEnd()))));
                }
            });
            setOnEditorActionListener((v, id, ev) -> {
                // (a keyboard's Enter comes down and up: once, on the way down)
                if (ev != null && ev.getAction() != KeyEvent.ACTION_DOWN) return true;
                if (game != 0) Native.typeKey(game, 1);
                // (sent: the box empty at once, not when the village says)
                setKb("");
                return true;
            });
        }

        @Override
        public boolean onKeyPreIme(int code, KeyEvent e) {
            // (the back key puts the keyboard away: the line's closed)
            if (code == KeyEvent.KEYCODE_BACK && e.getAction() == KeyEvent.ACTION_UP && kbOn) {
                kbOn = false;
                if (game != 0) Native.typeKey(game, 2);
                hideKeyboard();
                return true;
            }
            return super.onKeyPreIme(code, e);
        }
    }

    private void hideKeyboard() {
        android.view.inputmethod.InputMethodManager im = (android.view.inputmethod.InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
        if (im != null) {
            im.hideSoftInputFromWindow(kb.getWindowToken(), 0);
        }
        kb.clearFocus();
    }

    /** What the village says of a line being typed: "on\t<code|text>\t<text>", "set\t<text>", "off". */
    private void keyboard(String s) {
        String[] p = s.split("\t", -1);
        if ("on".equals(p[0])) {
            kbOn = true;
            kb.wasShown = false;
            boolean code = p.length > 1 && "code".equals(p[1]);
            kb.setInputType(android.text.InputType.TYPE_CLASS_TEXT | (code ? android.text.InputType.TYPE_TEXT_FLAG_CAP_CHARACTERS | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS : android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES));
            setKb(p.length > 2 ? p[2] : "");
            kb.requestFocus();
            android.view.inputmethod.InputMethodManager im = (android.view.inputmethod.InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
            if (im != null) {
                im.showSoftInput(kb, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);
            }
        } else if ("set".equals(p[0])) {
            // (the village's tidied the line, a code's dashes put in: taken
            // only if it's still what's in the box, give or take those;
            // typed on since, the village tidies that in its turn)
            String v = p.length > 1 ? p[1] : "";
            if (letters(v).equals(letters(kb.getText().toString()))) setKb(v);
        } else if ("off".equals(p[0])) {
            kbOn = false;
            hideKeyboard();
            immersive();
        }
    }

    private static String letters(String s) {
        return s.replace("-", "").replace(" ", "").toUpperCase(java.util.Locale.ROOT);
    }

    private void setKb(String text) {
        kbSetting = true;
        kb.setText(text);
        kb.setSelection(kb.getText().length());
        kbSetting = false;
    }

    private void typeLine(String question, String value) {
        // (reassigned below: a chat's title and its lines)
        // a chat: its title, then what's been said so far (a line each)
        String said = null;
        int nl = question.indexOf('\n');
        if (nl >= 0) {
            said = question.substring(nl + 1);
            question = question.substring(0, nl);
        }
        final android.widget.EditText e = new android.widget.EditText(this);
        e.setSingleLine(true);
        e.setHint(question);
        if (value != null && !value.isEmpty()) {
            e.setText(value);
            e.setSelection(value.length());
        }
        // a code asked for: capitals and a dash between each four, as it's
        // typed (a group's: then a name after a space, kept as it is)
        if (question.contains("XXXX-XXXX-XXXX")) {
            final boolean more = question.contains("name");
            e.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_CAP_CHARACTERS | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
            e.addTextChangedListener(new android.text.TextWatcher() {
                boolean busy;

                @Override
                public void beforeTextChanged(CharSequence s, int a, int b, int c) {}

                @Override
                public void onTextChanged(CharSequence s, int a, int b, int c) {}

                @Override
                public void afterTextChanged(android.text.Editable s) {
                    if (busy) return;
                    String v = typingCode(s.toString(), more);
                    if (!v.equals(s.toString())) {
                        busy = true;
                        s.replace(0, s.length(), v);
                        busy = false;
                    }
                }
            });
        }
        e.setTextColor(Color.WHITE);
        int pad = Math.round(16 * getResources().getDisplayMetrics().density);
        FrameLayout box = new FrameLayout(this);
        box.setPadding(pad, pad / 2, pad, 0);
        box.addView(e);
        AlertDialog.Builder ask = new AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert);
        if (said != null) {
            ask.setMessage(said);
        }
        ask
                .setTitle(question)
                .setView(box)
                .setPositiveButton("OK", (d, w) -> {
                    if (game != 0) Native.giveText(game, e.getText().toString());
                })
                .setNegativeButton("Never mind", (d, w) -> {
                    if (game != 0) Native.giveText(game, "");
                })
                .setOnCancelListener(d -> {
                    if (game != 0) Native.giveText(game, "");
                })
                .show();
        e.requestFocus();
    }

    /** What the village asks of the app (from its menus), and what's showing. */
    private void poll(final long g) {
        final String ask = Native.ask(g);
        final int st = Native.state(g);
        final boolean pockets = (st & 1) != 0;
        // the menu open: the joystick and buttons out of its way
        final boolean menu = (st & 4) != 0;
        if (ask == null && pockets == controls.pockets && menu == controls.menu) {
            return;
        }
        ui.post(new Runnable() {
            @Override
            public void run() {
                if (controls.menu != menu) {
                    controls.menu = menu;
                    controls.invalidate();
                }
                if (controls.pockets != pockets) {
                    controls.pockets = pockets;
                    controls.invalidate();
                }
                if ("wallpaper".equals(ask)) {
                    setWallpaper();
                } else if ("settings".equals(ask)) {
                    if (game != 0) Native.openPanel(game);
                } else if (ask != null && ask.startsWith("config:")) {
                    // the menu changed the settings: kept (the wallpaper reads them too)
                    String text = ask.substring(7);
                    Prefs.putConfig(GameActivity.this, text);
                    cfgUsed = text;
                    // its frame rate (Screen, or a graphics preset): the game's, the loop again at it
                    java.util.regex.Matcher m = java.util.regex.Pattern.compile("(?m)^fps = ([0-9.]+)").matcher(text);
                    if (m.find()) {
                        int f = 30;
                        try {
                            f = Math.max(15, Math.min(60, Math.round(Float.parseFloat(m.group(1)))));
                        } catch (NumberFormatException ignored) {
                        }
                        if (f != Prefs.get(GameActivity.this).getInt("game_fps", 30)) {
                            Prefs.get(GameActivity.this).edit().putInt("game_fps", f).apply();
                            if (running && f != loopFps) {
                                stopLoop();
                                startLoop();
                            }
                        }
                    }
                } else if (ask != null && ask.startsWith("edit:")) {
                    String q = ask.substring(5);
                    int k = q.indexOf('\u0001');
                    typeLine(k >= 0 ? q.substring(0, k) : q, k >= 0 ? q.substring(k + 1) : "");
                } else if (ask != null && ask.startsWith("open:https://github.com/")) {
                    // a page to read (the privacy page): the browser
                    try {
                        startActivity(new Intent(Intent.ACTION_VIEW, android.net.Uri.parse(ask.substring(5))));
                    } catch (Exception ignored) {
                    }
                } else if ("restart".equals(ask)) {
                    if (game != 0) Native.save(game);
                    recreate();
                } else if ("export".equals(ask) || "import".equals(ask)) {
                    if (game != 0) Native.save(game);
                    SettingsPanel.pickSave(GameActivity.this, "export".equals(ask));
                } else if ("fresh".equals(ask)) {
                    new AlertDialog.Builder(GameActivity.this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                            .setMessage("A new village? This one's pockets and scorecard go with it.")
                            .setPositiveButton("New village", (d, w) -> {
                                new java.io.File(getFilesDir(), "save.toml").delete();
                                restartGame();
                            })
                            .setNegativeButton("Keep this one", null)
                            .show();
                } else if ("uninstall".equals(ask)) {
                    Intent i = new Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:" + getPackageName()));
                    try {
                        startActivity(i);
                    } catch (Exception ignored) {
                    }
                } else if (ask != null && ask.startsWith("keyboard:")) {
                    keyboard(ask.substring(9));
                } else if (ask != null && ask.startsWith("text:")) {
                    typeLine(ask.substring(5), "");
                } else if (ask != null && ask.startsWith("rules:")) {
                    // the village's settings, a guest's change said yes to: kept
                    try {
                        org.json.JSONObject r = new org.json.JSONObject(ask.substring(6));
                        android.content.SharedPreferences.Editor e = Prefs.get(GameActivity.this).edit();
                        e.putInt("snow", (int) Math.round(r.optDouble("snow", 1) * 100));
                        e.putInt("wind", (int) Math.round(r.optDouble("wind", 1) * 100));
                        e.putInt("glow", (int) Math.round(r.optDouble("glow", 1) * 100));
                        e.putInt("events", (int) Math.round(r.optDouble("events", 1) * 100));
                        e.putInt("critters", (int) Math.round(r.optDouble("critters", 1) * 100));
                        e.putInt("chattiness", (int) Math.round(r.optDouble("chattiness", 1) * 100));
                        e.putInt("most_at_once", r.optInt("most_at_once", 24));
                        e.putInt("sleigh_minutes", (int) Math.round(r.optDouble("sleigh_minutes", 4)));
                        e.putBoolean("aurora", r.optBoolean("aurora", true));
                        e.putBoolean("bushes", r.optBoolean("bushes", true));
                        e.putString("time", r.optString("time", "auto"));
                        e.apply();
                        cfgUsed = cfgToml();
                    } catch (Exception ignored) {
                    }
                } else if (ask != null && ask.startsWith("you:")) {
                    // the villager changed in the village (or on a linked device)
                    Prefs.applyYou(GameActivity.this, ask.substring(4));
                } else if ("reload".equals(ask)) {
                    // another villager: theirs into the settings, and start again
                    try {
                        java.io.File f = new java.io.File(getFilesDir(), "save.toml");
                        byte[] b = new byte[(int) f.length()];
                        try (java.io.FileInputStream in = new java.io.FileInputStream(f)) {
                            int n = 0;
                            while (n < b.length) {
                                int r = in.read(b, n, b.length - n);
                                if (r < 0) break;
                                n += r;
                            }
                        }
                        Prefs.applyYou(GameActivity.this, Native.youJson(new String(b, "UTF-8")));
                    } catch (Exception ignored) {
                    }
                    recreate();
                } else if (ask != null && ask.startsWith("copy:")) {
                    String text = ask.substring(5);
                    ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                    if (cm != null) {
                        cm.setPrimaryClip(ClipData.newPlainText("xmas-tui", text));
                    }
                    android.widget.Toast.makeText(GameActivity.this, "copied: " + text, android.widget.Toast.LENGTH_SHORT).show();
                }
            }
        });
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
        if (game != 0) {
            Native.help(game);
        }
    }

    @Override
    public void onBackPressed() {
        if (settings != null) {
            closeSettings();
            return;
        }
        if (game != 0 && Native.back(game)) {
            return;
        }
        super.onBackPressed();
    }

    // ------------------------------------------------------------ settings

    /** The settings, over the village (as the desktop's panel is): changes show at once. */
    void openSettings() {
        if (settings != null) {
            return;
        }
        settings = new SettingsPanel(this, new SettingsPanel.Host() {
            @Override
            public void changed() {
                ui.removeCallbacks(applySettings);
                ui.postDelayed(applySettings, 150);
            }

            @Override
            public void close() {
                closeSettings();
            }

            @Override
            public void wallpaper() {
                setWallpaper();
            }

            @Override
            public void freshVillage() {
                restartGame();
            }
        }, true);
        root.addView(settings.view(), new FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT));
        settings.view().requestApplyInsets();
    }

    void closeSettings() {
        if (settings == null) {
            return;
        }
        // (a name being typed: the keyboard goes too)
        android.view.inputmethod.InputMethodManager im = (android.view.inputmethod.InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
        if (im != null) {
            im.hideSoftInputFromWindow(root.getWindowToken(), 0);
        }
        root.removeView(settings.view());
        settings = null;
        applySettings.run();
        immersive();
    }

    /** What was changed in the settings, taken in where the game stands. */
    private final Runnable applySettings = new Runnable() {
        @Override
        public void run() {
            ui.removeCallbacks(this);
            controls.rescale();
            if (game == 0) {
                return;
            }
            String cfg = cfgToml();
            if (!cfg.equals(cfgUsed)) {
                Native.configure(game, cfg, size());
                cfgUsed = cfg;
            }
            // a new frame rate: the loop again at it
            int fps = Math.max(15, Math.min(60, Prefs.get(GameActivity.this).getInt("game_fps", 30)));
            if (running && fps != loopFps) {
                stopLoop();
                startLoop();
            }
        }
    };

    /** A different save in place (imported, or a new village): the game starts again from it. */
    private void restartGame() {
        stopLoop();
        if (game != 0) {
            Native.forget(game);
            Native.destroy(game);
            game = 0;
        }
        SurfaceHolder h = surface.getHolder();
        if (h.getSurface() != null && h.getSurface().isValid() && surface.getWidth() > 0) {
            surfaceChanged(h, 0, surface.getWidth(), surface.getHeight());
        }
    }

    @Override
    protected void onActivityResult(int req, int res, Intent data) {
        if (SettingsPanel.onResult(this, req, res, data, this::restartGame)) {
            return;
        }
        super.onActivityResult(req, res, data);
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
        /** The game's menu is open (the controls hide, its touches are the menu's). */
        boolean menu;
        private final Paint fill = new Paint();
        /** A dp, times the buttons' size (Settings → Village). */
        private float d = getResources().getDisplayMetrics().density;
        /** One art pixel, in screen pixels. */
        private int P = Math.max(2, Math.round(3 * getResources().getDisplayMetrics().density));

        /** The buttons' size, as set (taken in again back from the settings). */
        void rescale() {
            float k = Math.max(60, Math.min(250, Prefs.get(GameActivity.this).getInt("btn_scale", 100))) / 100f;
            float dens = getResources().getDisplayMetrics().density;
            d = dens * k;
            P = Math.max(2, Math.round(3 * dens * k));
            invalidate();
        }
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
            rescale();
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
                case 7: return new float[] {w - 130 * d, 30 * d, 20 * d};
                default: return new float[] {w - 230 * d, h - 120 * d, 26 * d};
            }
        }

        private static final int N_BTN = 8;

        /** 7×7 pixel icons for the buttons ('#' lit, '+' bright). */
        private final String[][] icons = {
            {"#####..", "##.....", "##.....", "####...", "##.....", "##.....", "#####.."},
            {"...#...", "..###..", ".#####.", "#######", "..###..", "..###..", "..###.."},
            {"..###..", ".##+##.", "##+++##", "#######", "#######", ".#####.", "..###.."},
            {".......", "#######", ".......", "#######", ".......", "#######", "......."},
            {"..#.#..", ".#####.", "##...##", ".#...#.", "##...##", ".#####.", "..#.#.."},
            {"..###..", ".#...#.", "#..#..#", "#.###.#", "#..#..#", ".#...#.", "..###.."},
            {"...#...", "...#...", ".#####.", "#######", ".#####.", "...#...", "...#..."},
            {".......", "#######", "##...##", "#.#.#.#", "#..#..#", "#######", "......."},
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
            if (k <= 0f || menu) {
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
                case 4: if (game != 0) Native.openPanel(game); break;
                case 5: Native.button(game, Native.CENTRE); break;
                case 6: todo(); break;
                case 7: Native.online(game); break;
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
            // the menu open: every touch is the menu's (taps, slides, scrolling)
            if (menu) {
                if (idx == 0 || act == MotionEvent.ACTION_MOVE) {
                    int a = act == MotionEvent.ACTION_DOWN ? 0 : act == MotionEvent.ACTION_MOVE ? 1 : act == MotionEvent.ACTION_UP ? 2 : 3;
                    Native.touch(game, a, e.getX(0), e.getY(0));
                }
                return true;
            }
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
