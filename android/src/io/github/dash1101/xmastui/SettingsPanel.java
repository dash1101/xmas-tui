package io.github.dash1101.xmastui;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.text.Editable;
import android.text.InputType;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.List;

/**
 * The settings, as the desktop's panel: a dark panel edged in gold over the
 * village, its tabs along the top, each part headed in gold with a dotted
 * rule, ◂ ▸ to step a setting, candy-cane bars to slide one, [✓] boxes;
 * laid out for fingers (tall rows, big arrows). Every change is taken in
 * at once. The game shows it over the village; the wallpaper picker's
 * settings show it on a page of its own.
 */
final class SettingsPanel {
    static final int GOLD = Color.rgb(255, 211, 107);
    static final int TITLE = Color.rgb(255, 240, 208);
    static final int TEXT = Color.rgb(230, 220, 196);
    static final int DIM = Color.rgb(125, 122, 140);
    static final int BORDER = Color.rgb(201, 162, 90);
    static final int PANEL = Color.argb(245, 7, 10, 24);
    static final int HOT = Color.rgb(52, 64, 106);
    static final int CANDY_RED = Color.rgb(214, 48, 58);
    static final int CANDY_WHITE = Color.rgb(246, 240, 230);
    static final int PINE = Color.rgb(96, 190, 120);
    static final int EXPORT = 41, IMPORT = 42;
    static final String[] TABS = {"Village", "You", "Sound", "Looks", "Phone", "Save"};

    /** What the panel needs from where it's shown. */
    interface Host {
        /** A setting changed: take it in. */
        void changed();

        /** The ✕ (or the back button). */
        void close();

        /** Make the village the phone's wallpaper. */
        void wallpaper();

        /** A new village asked for (the save's gone): start it. */
        void freshVillage();
    }

    private final Activity a;
    private final Host host;
    private final boolean inGame;
    private final SharedPreferences prefs;
    private final FrameLayout root;
    private final LinearLayout panel;
    private LinearLayout tabBar, page, col;
    private ScrollView scroll;
    private int tab;
    /** The text, this much bigger (the menus' size setting). */
    private float k = 1f;

    SettingsPanel(Activity a, Host host, boolean inGame) {
        this.a = a;
        this.host = host;
        this.inGame = inGame;
        prefs = Prefs.get(a);
        tab = Math.max(0, Math.min(TABS.length - 1, prefs.getInt("settings_tab2", 0)));
        root = new FrameLayout(a);
        // over the village: a touch outside the panel puts it away (as a
        // click outside does on the desktop)
        root.setBackgroundColor(inGame ? Color.argb(70, 0, 0, 0) : Color.rgb(9, 12, 28));
        root.setClickable(true);
        root.setOnClickListener(v -> {
            if (this.inGame) host.close();
        });
        panel = new LinearLayout(a);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setClickable(true);
        panel.setBackground(box(PANEL, BORDER, 2));
        root.addView(panel);
        // (clear of the camera cutout and the system bars)
        root.setOnApplyWindowInsetsListener((v, in) -> {
            @SuppressWarnings("deprecation")
            int l = in.getSystemWindowInsetLeft(), t = in.getSystemWindowInsetTop(), r = in.getSystemWindowInsetRight(), b = in.getSystemWindowInsetBottom();
            root.setPadding(l, t, r, b);
            root.post(this::fitWidth);
            return in;
        });
        // (the phone turned, the window resized: the panel fitted again)
        root.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> {
            if (r - l != or - ol) root.post(this::fitWidth);
        });
        build();
    }

    View view() {
        return root;
    }

    /** Lay it all out again (the text size changed, a pet with other coats…). */
    void build() {
        k = Math.max(0.75f, Math.min(1.7f, Math.max(100, Math.min(250, prefs.getInt("ui_scale", 140))) / 140f));
        panel.removeAllViews();
        // as wide as suits a finger and an eye: the screen, short of a
        // margin, up to about a desktop panel's width
        FrameLayout.LayoutParams plp = new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
        plp.topMargin = dp(8);
        plp.bottomMargin = dp(8);
        panel.setLayoutParams(plp);
        fitWidth();

        // the title bar: ⚙ settings … ✕
        LinearLayout bar = new LinearLayout(a);
        bar.setGravity(Gravity.CENTER_VERTICAL);
        bar.setPadding(dp(12), dp(2), dp(2), 0);
        TextView t = mono("⚙ settings", 15, TITLE);
        bar.addView(t, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        if (!inGame) {
            bar.addView(button("▶ play", v -> a.startActivity(new Intent(a, GameActivity.class))));
            bar.addView(gap(dp(6)));
            bar.addView(button("❄ wallpaper", v -> host.wallpaper()));
        }
        TextView x = mono("✕", 18, DIM);
        x.setGravity(Gravity.CENTER);
        x.setOnClickListener(v -> host.close());
        bar.addView(x, new LinearLayout.LayoutParams(dp(48), dp(44)));
        panel.addView(bar);

        // the tabs (the chosen one lit gold), and a dotted rule under them
        HorizontalScrollView hs = new HorizontalScrollView(a);
        hs.setHorizontalScrollBarEnabled(false);
        tabBar = new LinearLayout(a);
        tabBar.setPadding(dp(8), 0, dp(8), 0);
        hs.addView(tabBar);
        panel.addView(hs);
        panel.addView(rule(), new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(6)));

        scroll = new ScrollView(a);
        scroll.setFillViewport(true);
        page = new LinearLayout(a);
        page.setOrientation(LinearLayout.VERTICAL);
        page.setPadding(dp(14), 0, dp(14), dp(16));
        // (a text field doesn't grab the keyboard as the page opens)
        page.setFocusableInTouchMode(true);
        scroll.addView(page);
        panel.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

        TextView foot = mono("◂ ▸ step  ·  drag along a bar  ·  changes show at once", 11, DIM);
        foot.setPadding(dp(14), dp(4), dp(14), dp(6));
        panel.addView(foot);
        showTab(tab);
    }

    private void showTab(int i) {
        // (outside the game: only the phone's own page; the village's
        // settings are in the game's menu)
        if (!inGame) {
            i = 4;
        }
        tab = i;
        prefs.edit().putInt("settings_tab2", i).apply();
        tabBar.removeAllViews();
        for (int n = 0; n < TABS.length; n++) {
            if (!inGame && n != 4) {
                continue;
            }
            final int which = n;
            TextView t = mono(TABS[n], 15, n == i ? GOLD : TEXT);
            t.setGravity(Gravity.CENTER);
            t.setPadding(dp(12), 0, dp(12), 0);
            if (n == i) t.setBackgroundColor(Color.rgb(64, 50, 18));
            t.setOnClickListener(v -> showTab(which));
            tabBar.addView(t, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)));
        }
        page.removeAllViews();
        switch (i) {
            case 0: village(); break;
            case 1: you(); break;
            case 2: sound(); break;
            case 3: looks(); break;
            case 4: phone(); break;
            default: save();
        }
        scroll.scrollTo(0, 0);
    }

    // ------------------------------------------------------------ the tabs

    private void village() {
        section("THE VILLAGE");
        choice("Time of day", "time", new String[] {"auto", "night", "dawn", "morning", "noon", "afternoon", "dusk"}, null, "auto");
        slider("Snowfall", "snow", 0, 300, 100, 25, "%");
        slider("Wind", "wind", 0, 300, 100, 25, "%");
        slider("Glow", "glow", 20, 200, 100, 10, "%");
        slider("Surprises", "events", 0, 300, 100, 25, "%");
        choiceInt("Santa's sleigh", "sleigh_minutes", new int[] {0, 1, 2, 3, 4, 6, 10, 15, 20}, new String[] {"never", "every ~1 min", "every ~2 min", "every ~3 min", "every ~4 min", "every ~6 min", "every ~10 min", "every ~15 min", "every ~20 min"}, 4);
        check("Northern lights", "aurora", true);
        check("Berry bushes (and birds)", "bushes", true);
        slider("Size", "size", 50, 200, sizeNow(), 5, "%");
        note("A bigger or smaller village. It stays the same size whichever way the phone's turned.");
        slider("Zoom", "zoom", 100, 400, 100, 10, "%");

        section("VISITORS");
        slider("Animals & people", "critters", 0, 300, 100, 25, "%");
        slider("Most at once", "most_at_once", 2, 40, 24, 1, "");
        slider("Chattiness", "chattiness", 0, 300, 100, 25, "%");
        slider("Speech bubbles", "talk_time", 50, 300, 100, 25, "%");
        note("(friends & family, who visit too, are in the You tab)");
    }

    private void you() {
        section("THE VILLAGER");
        text("Your name", "name", "neighbour", 16);
        text("Card's line", "motto", "(friends online see it)", 60);
        choice("Coat", "wear_coat", new String[] {"classic", "red", "green", "blue", "purple", "gold", "pink", "grey", "white"}, null, "classic");
        choice("Hat", "wear_hat", new String[] {"classic", "red", "green", "blue", "gold", "pink", "white", "none"}, null, "classic");
        choice("Hair", "wear_hair", new String[] {"classic", "brown", "dark", "blonde", "red", "grey"}, null, "classic");
        choice("Skin", "wear_skin", new String[] {"classic", "light", "fair", "tan", "deep"}, null, "classic");
        check("A scarf", "wear_scarf", true);
        slider("Walk speed", "walk_speed", 50, 300, 100, 25, "%");

        section("AUTOPILOT");
        check("Left alone, off doing their own thing", "autopilot", false);
        choice("Takes over", "auto_after", new String[] {"5", "10", "20", "30", "45", "60", "120", "300", "600", "1800", "3600"},
                new String[] {"after 5 s alone", "after 10 s alone", "after 20 s alone", "after 30 s alone", "after 45 s alone", "after 1 min alone", "after 2 min alone", "after 5 min alone", "after 10 min alone", "after 30 min alone", "after 1 h alone"}, "20");
        choice("Keeps to", "auto_area", new String[] {"anywhere", "home", "tree", "here"}, new String[] {"anywhere", "near home", "by the big tree", "where it took over"}, "anywhere");
        choice("That stretch", "auto_reach", new String[] {"0.5", "0.75", "1", "1.5", "2", "3", "4"},
                new String[] {"½ screen wide", "¾ screen wide", "1 screen wide", "1½ screens wide", "2 screens wide", "3 screens wide", "4 screens wide"}, "1");
        note("Snowmen and igloos, snowballs, the animals, chats, tag with the kids. Touch the joystick or a button to take over.");

        section("THE PET");
        String kind = prefs.getString("pet", "cat");
        choice("Pet", "pet", new String[] {"cat", "dog", "bunny", "none"}, null, "cat");
        if (!"none".equals(kind)) {
            String[] coats = "dog".equals(kind) ? new String[] {"golden", "chocolate", "black", "snowy", "spotty", "grey", "red"}
                    : "bunny".equals(kind) ? new String[] {"snowy", "brown", "grey", "black", "cream"}
                    : new String[] {"ginger", "black", "snowy", "tabby", "calico", "cream", "tuxedo"};
            choice("Coat", "pet_coat", coats, null, coats[0]);
            text("Pet's name", "pet_name", "(it asks)", 16);
        }

        section("ONLINE");
        choiceInt("Most visitors", "village_limit", new int[] {2, 3, 4, 6, 8, 10, 12, 16, 20, 24, 32, 48},
                new String[] {"2 at once", "3 at once", "4 at once", "6 at once", "8 at once", "10 at once", "12 at once", "16 at once", "20 at once", "24 at once", "32 at once", "48 at once"}, 8);
        note("The most in your village when it's open to friends (or listed in Public villages). The public squares take 16.");
        check("Star out strong language", "clean_words", true);
        note("In what others write online. Links are always hidden behind a button.");

        section("FRIENDS & FAMILY");
        note("Each comes by as a villager of their own (\"any\": picked from the name).");
        check("Only them (the animals still come)", "only_friends", false);
        final List<Prefs.Friend> fs = Prefs.friends(a);
        for (int i = 0; i < fs.size(); i++) {
            friend(fs, i);
        }
        if (fs.size() < 10) {
            LinearLayout r = new LinearLayout(a);
            r.setPadding(0, dp(6), 0, 0);
            r.addView(button("+ add someone", v -> {
                Prefs.Friend f = new Prefs.Friend();
                f.name = "Friend " + (fs.size() + 1);
                fs.add(f);
                Prefs.putFriends(a, fs);
                host.changed();
                showTab(1);
            }));
            col.addView(r);
        }
    }

    private void friend(final List<Prefs.Friend> fs, final int i) {
        final Prefs.Friend f = fs.get(i);
        final Runnable put = () -> {
            Prefs.putFriends(a, fs);
            host.changed();
        };
        LinearLayout r = row("♥", 0);
        EditText e = field(f.name, "name", 16);
        e.addTextChangedListener(watcher(v -> {
            f.name = v;
            put.run();
        }));
        r.addView(e, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        final TextView age = mono(f.kid ? " kid " : " grown-up ", 14, TEXT);
        age.setBackgroundColor(HOT);
        age.setGravity(Gravity.CENTER);
        age.setOnClickListener(v -> {
            f.kid = !f.kid;
            age.setText(f.kid ? " kid " : " grown-up ");
            put.run();
        });
        r.addView(gap(dp(8)));
        r.addView(age, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(36)));
        TextView del = mono("✕", 17, Color.rgb(230, 110, 110));
        del.setGravity(Gravity.CENTER);
        del.setOnClickListener(v -> {
            fs.remove(i);
            put.run();
            showTab(1);
        });
        r.addView(del, new LinearLayout.LayoutParams(dp(44), dp(44)));
        // their look: two to a line
        LinearLayout l1 = new LinearLayout(a), l2 = new LinearLayout(a);
        l1.addView(mini("coat", new String[] {"", "red", "green", "blue", "purple", "gold", "pink", "grey", "white"}, f.coat, v -> {
            f.coat = v;
            put.run();
        }));
        l1.addView(mini("hat", new String[] {"", "red", "green", "blue", "gold", "pink", "white", "none"}, f.hat, v -> {
            f.hat = v;
            put.run();
        }));
        l2.addView(mini("hair", new String[] {"", "brown", "dark", "blonde", "red", "grey"}, f.hair, v -> {
            f.hair = v;
            put.run();
        }));
        l2.addView(mini("skin", new String[] {"", "light", "fair", "tan", "deep"}, f.skin, v -> {
            f.skin = v;
            put.run();
        }));
        l1.setPadding(dp(22), 0, 0, 0);
        l2.setPadding(dp(22), 0, 0, dp(6));
        col.addView(l1);
        col.addView(l2);
    }

    private void sound() {
        section("SOUND");
        check("Sound in the game", "game_sound", true);
        slider("Volume", "vol_master", 0, 100, 50, 5, "%");
        slider("Music", "vol_music", 0, 100, 60, 5, "%");
        slider("Effects", "vol_sfx", 0, 100, 70, 5, "%");
        slider("Wind, fire & chimes", "vol_ambience", 0, 100, 50, 5, "%");
        choice("Music", "track", new String[] {"shuffle", "music-box", "celesta", "piano", "sleigh-ride"}, null, "shuffle");
    }

    private void looks() {
        section("SIZES");
        slider("Menus, windows & dialogs", "ui_scale", 100, 250, 140, 10, "%");
        note("The menus, conversations, cards, the games' table, this panel and the writing over the village; the village stays its own size.");
        slider("Buttons", "btn_scale", 60, 250, 100, 10, "%");
        note("The joystick and the buttons over the village.");
        section("PANELS & MENUS");
        choice("Look", "look", new String[] {"classic", "frosted", "wood", "gift"}, new String[] {"classic", "frosted", "wooden", "gift box"}, "classic");
        note("Classic, frosted with snow on top, a wooden frame, or a gift box tied up with a gold ribbon.");
        check("Speech bubbles (classic wallpaper)", "bubbles", true);
    }

    private void phone() {
        section("PLAYING");
        check("Hide the controls when not in use", "hide_controls", false);
        note("They fade after a few seconds untouched; a touch brings them back. A game controller works too.");
        choiceInt("Frame rate", "game_fps", new int[] {20, 24, 30, 45, 60}, null, 30);

        section("THE WALLPAPER");
        if (inGame) {
            LinearLayout r = new LinearLayout(a);
            r.setPadding(0, dp(4), 0, dp(4));
            r.addView(button("❄ make it the wallpaper", v -> host.wallpaper()));
            col.addView(r);
        }
        choice("It shows", "wall_style", new String[] {"classic", "village"}, new String[] {"the classic village", "the game's village"}, "classic");
        note("Classic: the snowy village with the dock below. The game's village: the villager off on autopilot, the same village and pockets as the game.");
        slider("Folk (classic; 0: as fit)", "classic_people", 0, 12, 0, 1, "");
        choiceInt("Frame rate", "fps", new int[] {10, 15, 20, 24, 30, 45, 60}, null, 24);
        note("Lower saves battery. It only runs while the home screen shows.");

        section("SNOWY DOCK (CLASSIC)");
        check("A shelf under the dock", "dock", true);
        choice("Style", "dock_style", new String[] {"frost", "wood", "candy", "stone"}, null, "frost");
        slider("Top edge", "dock_lift", 3, 45, 15, 1, "%");
        slider("Height", "dock_height", 2, 25, 9, 1, "%");
        slider("Width", "dock_width", 30, 100, 92, 2, "%");
        slider("Snow pile", "dock_pile", 0, 14, 6, 1, "px");
        note("Every launcher's dock sits somewhere different: slide the shelf under yours. Tap it to knock the snow off.");

        section("CONTROLS");
        note("Joystick: walk (all the way: run). E: talk, use, help build. Jump: hold to go higher, again in the air. ❄ snowball · ✦ things to do · ☰ pockets, invite along · ◎ back to the villager. Tap things; press and hold for what can be done there; two fingers look along the village.");
        section("UPKEEP");
        LinearLayout r = new LinearLayout(a);
        r.setPadding(0, dp(4), 0, dp(4));
        r.addView(button("⟲ reset settings", v -> new android.app.AlertDialog.Builder(a, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                .setMessage("Every setting back as it first was? The villager, their look, the pet, friends & family and the village stay.")
                .setPositiveButton("Reset them", (d, w) -> {
                    Prefs.resetKeepingOwn(a);
                    host.changed();
                    showTab(4);
                })
                .setNegativeButton("No", null)
                .show()));
        r.addView(gap(dp(8)));
        r.addView(button("⌫ uninstall…", v -> {
            // the system's own page for the app: its Uninstall asks first
            Intent i = new Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:" + a.getPackageName()));
            i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            try {
                a.startActivity(i);
            } catch (Exception e) {
                // (no such page on this phone)
            }
        }));
        col.addView(r);
        note("Uninstall opens the phone's page for the app (its Uninstall button asks first). Export the save first to keep the village.");
    }

    private void save() {
        section("THE SAVE");
        note("The village, the villager's pockets, stats and scorecard, their name, the pet, friends & family. The game and the wallpaper share it; the desktop and web versions read the same files.");
        File f = new File(a.getFilesDir(), "save.toml");
        note(f.exists() ? "Last saved: " + android.text.format.DateFormat.format("yyyy-MM-dd HH:mm", f.lastModified()) : "Not saved yet (play a while first).");
        LinearLayout r = new LinearLayout(a);
        r.setPadding(0, dp(4), 0, dp(4));
        r.addView(button("⇩ export save", v -> pick(true)));
        r.addView(gap(dp(8)));
        r.addView(button("⇧ import save", v -> pick(false)));
        col.addView(r);
        LinearLayout r2 = new LinearLayout(a);
        r2.setPadding(0, dp(8), 0, dp(4));
        r2.addView(button("↻ a new village", v -> new android.app.AlertDialog.Builder(a, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                .setMessage("A new village? This one's pockets and scorecard go with it.")
                .setPositiveButton("New village", (d, w) -> {
                    new File(a.getFilesDir(), "save.toml").delete();
                    host.freshVillage();
                    showTab(5);
                })
                .setNegativeButton("Keep this one", null)
                .show()));
        col.addView(r2);
    }

    // ------------------------------------------------------------ the save file

    private void pick(boolean export) {
        pickSave(a, export);
    }

    /** A save to export to, or to import (the game's menu asks too). */
    static void pickSave(Activity a, boolean export) {
        Intent i = new Intent(export ? Intent.ACTION_CREATE_DOCUMENT : Intent.ACTION_OPEN_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        i.setType(export ? "application/toml" : "*/*");
        if (export) {
            i.putExtra(Intent.EXTRA_TITLE, "xmas-tui-save-" + android.text.format.DateFormat.format("yyyy-MM-dd", System.currentTimeMillis()) + ".toml");
        }
        try {
            a.startActivityForResult(i, export ? EXPORT : IMPORT);
        } catch (ActivityNotFoundException e) {
            toast(a, "no file picker on this phone");
        }
    }

    /**
     * A save picked to export to or import from (the activity passes its
     * results on). True if it was one of ours; `imported` runs once a save's
     * been put in place.
     */
    static boolean onResult(Activity a, int req, int res, Intent data, Runnable imported) {
        if (req != EXPORT && req != IMPORT) return false;
        if (res != Activity.RESULT_OK || data == null || data.getData() == null) return true;
        Uri uri = data.getData();
        File f = new File(a.getFilesDir(), "save.toml");
        try {
            if (req == EXPORT) {
                byte[] b = read(new FileInputStream(f));
                try (OutputStream out = a.getContentResolver().openOutputStream(uri)) {
                    if (out != null) out.write(b);
                }
                toast(a, "saved a copy");
            } else {
                InputStream in = a.getContentResolver().openInputStream(uri);
                if (in == null) return true;
                String text = new String(read(in), "UTF-8");
                if (Native.saveSeed(text) == 0) {
                    toast(a, "that isn't an xmas-tui save");
                    return true;
                }
                try (FileOutputStream out = new FileOutputStream(f)) {
                    out.write(text.getBytes("UTF-8"));
                }
                // the villager too: their name, the pet, friends & family
                Prefs.applyYou(a, Native.youJson(text));
                toast(a, "imported: the village, the villager and their things");
                if (imported != null) imported.run();
            }
        } catch (Exception e) {
            toast(a, "couldn't: " + e.getMessage());
        }
        return true;
    }

    private static byte[] read(InputStream in) throws java.io.IOException {
        try (InputStream i = in) {
            ByteArrayOutputStream o = new ByteArrayOutputStream();
            byte[] buf = new byte[8192];
            int n;
            while ((n = i.read(buf)) > 0) o.write(buf, 0, n);
            return o.toByteArray();
        }
    }

    static void toast(Context c, String s) {
        Toast.makeText(c, s, Toast.LENGTH_SHORT).show();
    }

    // ------------------------------------------------------------ pieces

    /** The size setting, or what the older "detail" one comes to. */
    private int sizeNow() {
        int legacy = Math.round(155f * 100f / Math.max(60, prefs.getInt("detail", 155)));
        return Math.max(50, Math.min(200, prefs.getInt("size", legacy)));
    }

    private int dp(float v) {
        return Math.round(TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, a.getResources().getDisplayMetrics()));
    }

    private static GradientDrawable box(int fill, int edge, int radius, Context c) {
        float d = c.getResources().getDisplayMetrics().density;
        GradientDrawable g = new GradientDrawable();
        g.setColor(fill);
        g.setCornerRadius(radius * d);
        g.setStroke(Math.max(1, Math.round(d)), edge);
        return g;
    }

    private GradientDrawable box(int fill, int edge, int radius) {
        return box(fill, edge, radius, a);
    }

    private View gap(int w) {
        View v = new View(a);
        v.setLayoutParams(new LinearLayout.LayoutParams(w, 1));
        return v;
    }

    /** Writing in the panel's typewriter letters, at the menus' size. */
    private TextView mono(String s, float sp, int color) {
        TextView t = new TextView(a);
        t.setText(s);
        t.setTextSize(sp * k);
        t.setTextColor(color);
        t.setTypeface(Typeface.MONOSPACE);
        return t;
    }

    /** A dotted rule (under the tabs, along a heading). */
    private View rule() {
        View v = new View(a) {
            final Paint p = new Paint();

            @Override
            protected void onDraw(Canvas c) {
                p.setColor(Color.argb(115, 201, 162, 90));
                float step = dp(6), y = getHeight() / 2f;
                for (float x = dp(2); x < getWidth() - dp(2); x += step) {
                    c.drawRect(x, y, x + dp(3), y + Math.max(1, dp(1)), p);
                }
            }
        };
        return v;
    }

    /** A heading: gold capitals and a dotted rule along to the right. */
    private void section(String s) {
        col = page;
        LinearLayout r = new LinearLayout(a);
        r.setGravity(Gravity.CENTER_VERTICAL);
        r.setPadding(0, dp(14), 0, dp(2));
        r.addView(mono(s, 14, GOLD));
        View line = rule();
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(0, dp(6), 1f);
        lp.leftMargin = dp(8);
        r.addView(line, lp);
        page.addView(r);
    }

    private void note(String s) {
        TextView t = mono(s, 12, DIM);
        t.setPadding(0, dp(2), 0, dp(6));
        col.addView(t);
    }

    /** A row: its label (a column wide), then what sets it. */
    private LinearLayout row(String label, int labelDp) {
        LinearLayout r = new LinearLayout(a);
        r.setGravity(Gravity.CENTER_VERTICAL);
        r.setMinimumHeight(dp(44));
        TextView t = mono(label, 15, TEXT);
        if (labelDp > 0) {
            r.addView(t, new LinearLayout.LayoutParams(dp(labelDp * Math.min(1.3f, k)), ViewGroup.LayoutParams.WRAP_CONTENT));
        } else {
            t.setPadding(0, 0, dp(8), 0);
            r.addView(t);
        }
        col.addView(r);
        return r;
    }

    /** A ◂ or ▸: a big target for a finger. */
    private TextView arrow(String s, View.OnClickListener on) {
        TextView t = mono(s, 18, BORDER);
        t.setGravity(Gravity.CENTER);
        t.setOnClickListener(on);
        t.setLayoutParams(new LinearLayout.LayoutParams(dp(40), dp(44)));
        return t;
    }

    private TextView button(String s, View.OnClickListener on) {
        TextView b = mono(" " + s + " ", 14, TEXT);
        b.setGravity(Gravity.CENTER);
        b.setPadding(dp(8), 0, dp(8), 0);
        b.setBackground(box(HOT, Color.rgb(90, 104, 150), 2));
        b.setMinHeight(dp(40));
        b.setOnClickListener(on);
        return b;
    }

    /** [✓] and what it is: the whole line toggles it. */
    private void check(String label, final String key, boolean def) {
        final boolean[] on = {prefs.getBoolean(key, def)};
        LinearLayout r = new LinearLayout(a);
        r.setGravity(Gravity.CENTER_VERTICAL);
        r.setMinimumHeight(dp(44));
        final TextView mark = mono(on[0] ? "[✓]" : "[ ]", 15, BORDER);
        final TextView t = mono(" " + label, 15, on[0] ? TEXT : DIM);
        r.addView(mark);
        r.addView(t);
        r.setOnClickListener(v -> {
            on[0] = !on[0];
            prefs.edit().putBoolean(key, on[0]).apply();
            mark.setText(on[0] ? "[✓]" : "[ ]");
            t.setTextColor(on[0] ? TEXT : DIM);
            host.changed();
            // (autopilot's own settings show with it)
            if (key.equals("autopilot")) showTab(tab);
        });
        col.addView(r);
    }

    interface Got {
        void got(String v);
    }

    private TextWatcher watcher(final Got g) {
        return new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int a, int b, int c) {}

            @Override
            public void onTextChanged(CharSequence s, int a, int b, int c) {}

            @Override
            public void afterTextChanged(Editable s) {
                g.got(s.toString().trim());
            }
        };
    }

    private EditText field(String value, String hint, int max) {
        EditText e = new EditText(a);
        e.setSingleLine(true);
        e.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_WORDS);
        e.setFilters(new android.text.InputFilter[] {new android.text.InputFilter.LengthFilter(max)});
        e.setHint(hint);
        e.setTextSize(15 * k);
        e.setTypeface(Typeface.MONOSPACE);
        e.setTextColor(TITLE);
        e.setHintTextColor(DIM);
        e.setText(value);
        e.setPadding(dp(8), dp(4), dp(8), dp(4));
        e.setBackground(box(Color.rgb(16, 22, 46), Color.rgb(74, 84, 128), 2));
        return e;
    }

    private void text(String label, final String key, String hint, final int max) {
        LinearLayout r = row(label, 130);
        EditText e = field(prefs.getString(key, ""), hint, max);
        e.addTextChangedListener(watcher(v -> {
            prefs.edit().putString(key, v).apply();
            host.changed();
        }));
        r.addView(e, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
    }

    /** ◂ value ▸: the arrows (or the value itself) step through the choices. */
    private void choice(String label, final String key, final String[] values, String[] words, String def) {
        final String[] shown = words != null ? words : values;
        int cur = 0;
        String now = prefs.getString(key, def);
        for (int i = 0; i < values.length; i++) if (values[i].equals(now)) cur = i;
        final int[] at = {cur};
        LinearLayout r = row(label, 130);
        final TextView val = mono(shown[cur], 15, TITLE);
        val.setGravity(Gravity.CENTER_VERTICAL);
        final Runnable set = () -> {
            val.setText(shown[at[0]]);
            prefs.edit().putString(key, values[at[0]]).apply();
            host.changed();
            // a different pet has different coats
            if (key.equals("pet")) showTab(tab);
        };
        r.addView(arrow("◂", v -> {
            at[0] = (at[0] + values.length - 1) % values.length;
            set.run();
        }));
        val.setOnClickListener(v -> {
            at[0] = (at[0] + 1) % values.length;
            set.run();
        });
        r.addView(val, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)));
        r.addView(arrow("▸", v -> {
            at[0] = (at[0] + 1) % values.length;
            set.run();
        }));
    }

    /** A choice of numbers (stored as a number), in words or as frames a second. */
    private void choiceInt(String label, final String key, final int[] values, String[] words, int def) {
        final String[] shown = new String[values.length];
        int cur = 0, now = prefs.getInt(key, def);
        for (int i = 0; i < values.length; i++) {
            shown[i] = words != null ? words[i] : values[i] + " fps";
            if (values[i] == now) cur = i;
        }
        final int[] at = {cur};
        LinearLayout r = row(label, 130);
        final TextView val = mono(shown[cur], 15, TITLE);
        val.setGravity(Gravity.CENTER_VERTICAL);
        final Runnable set = () -> {
            val.setText(shown[at[0]]);
            prefs.edit().putInt(key, values[at[0]]).apply();
            host.changed();
        };
        r.addView(arrow("◂", v -> {
            at[0] = Math.max(0, at[0] - 1);
            set.run();
        }));
        val.setOnClickListener(v -> {
            at[0] = (at[0] + 1) % values.length;
            set.run();
        });
        r.addView(val, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(44)));
        r.addView(arrow("▸", v -> {
            at[0] = Math.min(values.length - 1, at[0] + 1);
            set.run();
        }));
    }

    /** A friend's coat, hat…: "any" stores "". */
    private View mini(String label, final String[] values, String cur, final Got g) {
        LinearLayout r = new LinearLayout(a);
        r.setGravity(Gravity.CENTER_VERTICAL);
        r.addView(mono(label, 13, DIM));
        int sel = 0;
        for (int i = 0; i < values.length; i++) if (values[i].equals(cur == null ? "" : cur)) sel = i;
        final int[] at = {sel};
        final TextView val = mono(values[sel].isEmpty() ? "any" : values[sel], 14, TITLE);
        val.setGravity(Gravity.CENTER_VERTICAL);
        final Runnable set = () -> {
            val.setText(values[at[0]].isEmpty() ? "any" : values[at[0]]);
            g.got(values[at[0]]);
        };
        r.addView(arrow("◂", v -> {
            at[0] = (at[0] + values.length - 1) % values.length;
            set.run();
        }));
        r.addView(val, new LinearLayout.LayoutParams(dp(64 * Math.min(1.3f, k)), dp(44)));
        r.addView(arrow("▸", v -> {
            at[0] = (at[0] + 1) % values.length;
            set.run();
        }));
        r.setLayoutParams(new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        return r;
    }

    /** Label ◂ candy-cane bar ▸ value: the arrows step, a finger slides it. */
    private void slider(String label, final String key, final int min, final int max, int def, final int step, final String unit) {
        final int[] v = {Math.max(min, Math.min(max, prefs.getInt(key, def)))};
        // (on a narrow screen: the label above, the bar under it)
        boolean narrow = panelWidthDp() < 520;
        LinearLayout r;
        if (narrow) {
            TextView t = mono(label, 15, TEXT);
            t.setPadding(0, dp(6), 0, 0);
            col.addView(t);
            r = new LinearLayout(a);
            r.setGravity(Gravity.CENTER_VERTICAL);
            col.addView(r);
        } else {
            r = row(label, 190);
        }
        final TextView val = mono(v[0] + unit, 15, TITLE);
        val.setGravity(Gravity.CENTER_VERTICAL | Gravity.END);
        final Candy bar = new Candy(a);
        bar.frac = (v[0] - min) / (float) Math.max(1, max - min);
        final Runnable set = () -> {
            val.setText(v[0] + unit);
            bar.frac = (v[0] - min) / (float) Math.max(1, max - min);
            bar.invalidate();
            prefs.edit().putInt(key, v[0]).apply();
            host.changed();
        };
        bar.moved = (f, done) -> {
            int nv = Math.max(min, Math.min(max, Math.round(min + f * (max - min))));
            if (nv != v[0]) {
                v[0] = nv;
                set.run();
            }
            // (the panel's own letters grow with the menus' size)
            if (done && key.equals("ui_scale")) build();
        };
        r.addView(arrow("◂", x -> {
            v[0] = Math.max(min, v[0] - step);
            set.run();
            if (key.equals("ui_scale")) build();
        }));
        r.addView(bar, new LinearLayout.LayoutParams(0, dp(44), 1f));
        r.addView(arrow("▸", x -> {
            v[0] = Math.min(max, v[0] + step);
            set.run();
            if (key.equals("ui_scale")) build();
        }));
        r.addView(val, new LinearLayout.LayoutParams(dp(62 * Math.min(1.3f, k)), dp(44)));
    }

    /** The room there is across (inside the cutouts and bars). */
    private int room() {
        int w = root.getWidth() - root.getPaddingLeft() - root.getPaddingRight();
        return w > 0 ? w : a.getResources().getDisplayMetrics().widthPixels - root.getPaddingLeft() - root.getPaddingRight();
    }

    /** The panel's sides: a margin, or more on a wide screen (a desktop panel's width at most). */
    private void fitWidth() {
        int side = Math.max(dp(8), (room() - dp(720 * Math.min(1.25f, k))) / 2);
        FrameLayout.LayoutParams lp = (FrameLayout.LayoutParams) panel.getLayoutParams();
        if (lp != null && lp.leftMargin != side) {
            lp.leftMargin = side;
            lp.rightMargin = side;
            panel.setLayoutParams(lp);
        }
    }

    private float panelWidthDp() {
        float d = a.getResources().getDisplayMetrics().density;
        return Math.min(room() - dp(16), dp(720 * Math.min(1.25f, k))) / d;
    }

    interface Moved {
        void moved(float frac, boolean done);
    }

    /** The desktop's candy-cane bar: ▰ red and white for how far, ▱ the rest. */
    final class Candy extends View {
        float frac;
        Moved moved;
        private final Paint p = new Paint();

        Candy(Context c) {
            super(c);
        }

        @Override
        protected void onDraw(Canvas c) {
            int n = 20;
            float w = getWidth() - dp(4), h = Math.min(getHeight() * 0.42f, dp(12));
            float cell = w / n, y = (getHeight() - h) / 2f;
            int lit = Math.round(Math.max(0f, Math.min(1f, frac)) * n);
            for (int i = 0; i < n; i++) {
                float x = dp(2) + i * cell;
                if (i < lit) {
                    p.setStyle(Paint.Style.FILL);
                    p.setColor(i % 2 == 0 ? CANDY_RED : CANDY_WHITE);
                    c.drawRect(x + cell * 0.08f, y, x + cell * 0.92f, y + h, p);
                } else {
                    p.setStyle(Paint.Style.STROKE);
                    p.setStrokeWidth(Math.max(1, dp(1)));
                    p.setColor(Color.argb(170, 125, 122, 140));
                    c.drawRect(x + cell * 0.12f, y + 1, x + cell * 0.88f, y + h - 1, p);
                }
            }
        }

        @Override
        public boolean onTouchEvent(MotionEvent e) {
            int act = e.getActionMasked();
            if (act == MotionEvent.ACTION_DOWN) {
                // (a slide along the bar, not the page scrolling)
                getParent().requestDisallowInterceptTouchEvent(true);
            }
            if (act == MotionEvent.ACTION_DOWN || act == MotionEvent.ACTION_MOVE || act == MotionEvent.ACTION_UP) {
                float f = (e.getX() - dp(2)) / Math.max(1f, getWidth() - dp(4));
                f = Math.max(0f, Math.min(1f, f));
                frac = f;
                invalidate();
                if (moved != null) moved.moved(f, act == MotionEvent.ACTION_UP);
                return true;
            }
            return act == MotionEvent.ACTION_CANCEL || super.onTouchEvent(e);
        }
    }
}
