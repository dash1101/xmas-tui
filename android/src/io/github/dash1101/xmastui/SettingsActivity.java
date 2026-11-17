package io.github.dash1101.xmastui;

import android.app.Activity;
import android.app.WallpaperManager;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Bundle;
import android.text.Editable;
import android.text.InputType;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.AdapterView;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.EditText;
import android.widget.HorizontalScrollView;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.Spinner;
import android.widget.Switch;
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
 * The settings: the village, friends & family, sound and looks, playing,
 * the wallpaper, the save. Laid out as the desktop's are, a tab at a time,
 * each part on a card.
 */
public class SettingsActivity extends Activity {
    private static final int GOLD = Color.rgb(255, 211, 107);
    private static final int TEXT = Color.rgb(236, 232, 220);
    private static final int DIM = Color.rgb(150, 160, 190);
    private static final int PANEL = Color.rgb(20, 26, 52);
    private static final int BACKDROP = Color.rgb(9, 12, 28);
    private static final int EXPORT = 1, IMPORT = 2;
    private static final String[] TABS = {"Village", "Friends", "Sound & looks", "Playing", "Wallpaper", "Save"};

    private SharedPreferences prefs;
    /** The page (the tab's cards go in it), and the card being filled. */
    private LinearLayout page, col;
    private LinearLayout tabBar;
    private int tab;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        prefs = Prefs.get(this);
        tab = prefs.getInt("settings_tab", 0);
        ScrollView scroll = new ScrollView(this);
        scroll.setBackgroundColor(BACKDROP);
        final LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        final int pad = dp(16);
        root.setPadding(pad, pad, pad, pad + dp(56));
        // (Android 15 draws apps edge to edge: keep clear of the system bars)
        scroll.setOnApplyWindowInsetsListener((v, insets) -> {
            @SuppressWarnings("deprecation")
            int top = insets.getSystemWindowInsetTop(), bottom = insets.getSystemWindowInsetBottom();
            root.setPadding(pad, pad + top, pad, pad + dp(24) + bottom);
            return insets;
        });
        // (so a text field doesn't grab focus and scroll the page down)
        root.setFocusableInTouchMode(true);
        root.requestFocus();
        scroll.addView(root);
        setContentView(scroll);

        // the heading: the icon, the name, the two big buttons
        LinearLayout head = new LinearLayout(this);
        head.setGravity(Gravity.CENTER_VERTICAL);
        ImageView icon = new ImageView(this);
        icon.setImageResource(R.mipmap.ic_launcher);
        head.addView(icon, new LinearLayout.LayoutParams(dp(44), dp(44)));
        TextView title = label("xmas-tui village", 21, GOLD);
        title.setPadding(dp(12), 0, 0, 0);
        title.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        head.addView(title);
        root.addView(head);
        LinearLayout acts = new LinearLayout(this);
        acts.setPadding(0, dp(12), 0, dp(4));
        acts.addView(bigButton("▶  Play", v -> startActivity(new Intent(this, GameActivity.class))), new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        View gap = new View(this);
        acts.addView(gap, new LinearLayout.LayoutParams(dp(10), 1));
        acts.addView(bigButton("❄  Wallpaper", v -> setWallpaper()), new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        root.addView(acts);

        // the tabs
        HorizontalScrollView hs = new HorizontalScrollView(this);
        hs.setHorizontalScrollBarEnabled(false);
        tabBar = new LinearLayout(this);
        tabBar.setPadding(0, dp(12), 0, dp(4));
        hs.addView(tabBar);
        root.addView(hs);
        page = new LinearLayout(this);
        page.setOrientation(LinearLayout.VERTICAL);
        root.addView(page);
        showTab(tab);
    }

    private void showTab(int i) {
        tab = i;
        prefs.edit().putInt("settings_tab", i).apply();
        tabBar.removeAllViews();
        for (int k = 0; k < TABS.length; k++) {
            final int which = k;
            TextView t = label(TABS[k], 14, k == i ? Color.rgb(20, 22, 40) : TEXT);
            t.setTypeface(Typeface.MONOSPACE, k == i ? Typeface.BOLD : Typeface.NORMAL);
            t.setPadding(dp(12), dp(7), dp(12), dp(7));
            t.setBackground(rounded(k == i ? GOLD : PANEL, k == i ? GOLD : Color.rgb(60, 70, 110), 16));
            t.setOnClickListener(v -> showTab(which));
            LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
            lp.rightMargin = dp(6);
            tabBar.addView(t, lp);
        }
        page.removeAllViews();
        switch (i) {
            case 0: village(); break;
            case 1: friends(); break;
            case 2: soundAndLooks(); break;
            case 3: playing(); break;
            case 4: wallpaper(); break;
            default: save();
        }
    }

    // ------------------------------------------------------------ the tabs

    private void village() {
        card("THE VILLAGER");
        text("Name", "name", "neighbour", 16);
        text("Card's line", "motto", "(friends online see it)", 60);
        choice("Your coat", "wear_coat", new String[] {"classic", "red", "green", "blue", "purple", "gold", "pink", "grey", "white"}, "classic");
        choice("Your hat", "wear_hat", new String[] {"classic", "red", "green", "blue", "gold", "pink", "white", "none"}, "classic");
        choice("Your hair", "wear_hair", new String[] {"classic", "brown", "dark", "blonde", "red", "grey"}, "classic");
        choice("Your skin", "wear_skin", new String[] {"classic", "light", "fair", "tan", "deep"}, "classic");
        toggle("A scarf", "wear_scarf", true);
        choice("Pet", "pet", new String[] {"cat", "dog", "bunny", "none"}, "cat");
        String kind = prefs.getString("pet", "cat");
        String[] coats = "dog".equals(kind) ? new String[] {"golden", "chocolate", "black", "snowy", "spotty", "grey", "red"}
                : "bunny".equals(kind) ? new String[] {"snowy", "brown", "grey", "black", "cream"}
                : new String[] {"ginger", "black", "snowy", "tabby", "calico", "cream", "tuxedo"};
        choice("Pet's coat", "pet_coat", coats, coats[0]);
        text("Pet's name", "pet_name", "(it asks)", 16);
        slider("Walking speed", "walk_speed", 50, 300, 100, "%");

        card("THE VILLAGE");
        choice("Time of day", "time", new String[] {"auto", "night", "dawn", "morning", "noon", "afternoon", "dusk"}, "auto");
        slider("Snowfall", "snow", 0, 300, 100, "%");
        slider("Wind", "wind", 0, 300, 100, "%");
        toggle("Northern lights", "aurora", true);
        slider("Glow of the lights", "glow", 20, 200, 100, "%");
        slider("Size", "size", 50, 200, sizeNow(), "%");
        note("Bigger or smaller village. It stays the same size whichever way the screen is turned: sideways shows more of it.");
        slider("Zoom", "zoom", 100, 300, 100, "%");

        card("FOLK & HAPPENINGS");
        slider("Animals and people coming by", "critters", 0, 300, 100, "%");
        slider("The most out at once", "most_at_once", 2, 40, 24, "");
        toggle("Berry bushes (and birds)", "bushes", true);
        slider("Surprises (shooting stars, light shows…)", "events", 0, 300, 100, "%");
        slider("Santa's sleigh: every", "sleigh_minutes", 0, 20, 4, " min");
        slider("Chattiness", "chattiness", 0, 300, 100, "%");
        slider("Speech bubbles stay up", "talk_time", 50, 300, 100, "%");
    }

    private void friends() {
        card("FRIENDS & FAMILY");
        note("Some of the folk who come by take these names, know the villager, and wear what's chosen for them. Anything left on \"any\" is picked from the name.");
        toggle("Only them (no strangers; the animals still come)", "only_friends", false);
        final List<Prefs.Friend> fs = Prefs.friends(this);
        for (int i = 0; i < fs.size(); i++) {
            friendRow(fs, i);
        }
        Button add = bigButton("+  Add someone", v -> {
            fs.add(new Prefs.Friend());
            fs.get(fs.size() - 1).name = "Friend " + fs.size();
            Prefs.putFriends(this, fs);
            showTab(1);
        });
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.topMargin = dp(10);
        col.addView(add, lp);
    }

    private void friendRow(final List<Prefs.Friend> fs, final int i) {
        final Prefs.Friend f = fs.get(i);
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(12), dp(8), dp(12), dp(8));
        box.setBackground(rounded(Color.rgb(28, 36, 70), Color.rgb(70, 80, 130), 10));
        LinearLayout.LayoutParams blp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        blp.topMargin = dp(8);
        col.addView(box, blp);
        LinearLayout top = new LinearLayout(this);
        top.setGravity(Gravity.CENTER_VERTICAL);
        EditText name = new EditText(this);
        name.setSingleLine(true);
        name.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_WORDS);
        name.setText(f.name);
        name.setTextColor(Color.WHITE);
        name.addTextChangedListener(watcher(v -> {
            f.name = v.length() > 16 ? v.substring(0, 16) : v;
            Prefs.putFriends(this, fs);
        }));
        top.addView(name, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        TextView kidL = label("kid", 14, DIM);
        kidL.setPadding(dp(8), 0, dp(4), 0);
        top.addView(kidL);
        Switch kid = new Switch(this);
        kid.setChecked(f.kid);
        kid.setOnCheckedChangeListener((b, on) -> {
            f.kid = on;
            Prefs.putFriends(this, fs);
        });
        top.addView(kid);
        TextView del = label("  ✕", 18, Color.rgb(230, 110, 110));
        del.setOnClickListener(v -> {
            fs.remove(i);
            Prefs.putFriends(this, fs);
            showTab(1);
        });
        top.addView(del);
        box.addView(top);
        LinearLayout looks = new LinearLayout(this);
        looks.setGravity(Gravity.CENTER_VERTICAL);
        looks.addView(miniChoice("coat", new String[] {"any", "red", "green", "blue", "purple", "gold", "pink", "grey", "white"}, f.coat, v -> {
            f.coat = v;
            Prefs.putFriends(this, fs);
        }));
        looks.addView(miniChoice("hat", new String[] {"any", "red", "green", "blue", "gold", "pink", "white", "none"}, f.hat, v -> {
            f.hat = v;
            Prefs.putFriends(this, fs);
        }));
        box.addView(looks);
        LinearLayout more = new LinearLayout(this);
        more.setGravity(Gravity.CENTER_VERTICAL);
        more.addView(miniChoice("hair", new String[] {"any", "brown", "dark", "blonde", "red", "grey"}, f.hair, v -> {
            f.hair = v;
            Prefs.putFriends(this, fs);
        }));
        more.addView(miniChoice("skin", new String[] {"any", "light", "fair", "tan", "deep"}, f.skin, v -> {
            f.skin = v;
            Prefs.putFriends(this, fs);
        }));
        box.addView(more);
    }

    private void soundAndLooks() {
        card("SOUND");
        toggle("Sound in the game", "game_sound", true);
        slider("Volume", "vol_master", 0, 100, 50, "%");
        slider("Music", "vol_music", 0, 100, 60, "%");
        slider("Effects", "vol_sfx", 0, 100, 70, "%");
        slider("Wind, fire & chimes", "vol_ambience", 0, 100, 50, "%");
        choice("Music", "track", new String[] {"shuffle", "music-box", "celesta", "piano", "sleigh-ride"}, "shuffle");

        card("PANELS & MENUS");
        choice("Their look", "look", new String[] {"classic", "frosted", "wood", "gift"}, "classic");
        note("The menus, conversations, the games' table and the cards in the village: classic, frosted with snow on top, a wooden frame, or a gift box tied up with a gold ribbon.");
        toggle("Speech bubbles (classic wallpaper)", "bubbles", true);
    }

    private void playing() {
        card("PLAYING IN THE VILLAGE");
        toggle("Autopilot", "autopilot", false);
        note("Left alone a while, the villager goes about their own day: snowmen and igloos, snowballs, feeding the animals, chats, tag with the kids. Touch the joystick or a button to take over.");
        toggle("Hide the controls when not in use", "hide_controls", false);
        note("They fade away after a few seconds untouched; touch the screen and they're back. A game controller works too.");
        choiceInt("Frames per second", "game_fps", new int[] {20, 24, 30, 45, 60}, 30);
        card("CONTROLS");
        note("Joystick: walk (all the way: run); up and down choose in menus and conversations.\nE: talk, use, help build; picks in menus.\nJump: hold to go higher; in the air, again, and the Christmas spirit lifts the villager once more.\n❄ snowball · ✦ things to do · ☰ pockets, invite along, lie down · ◎ back to the villager.\nTap things; press and hold for what can be done there; drag people, animals and the pet; two fingers look along the village. Menus and cards move by their title bars.");
    }

    private void wallpaper() {
        card("THE WALLPAPER");
        choice("The wallpaper shows", "wall_style", new String[] {"classic", "village"}, "classic");
        note("classic: the snowy village with the dock below, its folk taking the friends & family names and keeping to the most-at-once. village: the game's own village with the villager off on autopilot, the same village and pockets as the game, so one carries on from the other.");
        slider("Folk on the classic wallpaper (0: as many as fit)", "classic_people", 0, 12, 0, "");
        choiceInt("Frames per second", "fps", new int[] {10, 15, 20, 24, 30, 45, 60}, 24);
        note("Lower saves battery. It only runs while the home screen is showing.");

        card("SNOWY DOCK (CLASSIC)");
        toggle("Show a shelf under the dock", "dock", true);
        choice("Style", "dock_style", new String[] {"frost", "wood", "candy", "stone"}, "frost");
        slider("Top edge (from the bottom)", "dock_lift", 3, 45, 15, "%");
        slider("Height", "dock_height", 2, 25, 9, "%");
        slider("Width", "dock_width", 30, 100, 92, "%");
        slider("Snow pile", "dock_pile", 0, 14, 6, "px");
        note("Every launcher puts its dock somewhere different: slide the shelf to sit under yours. Snow lands on it and piles up; tap it to knock the snow off.");
    }

    private void save() {
        card("THE SAVE");
        note("The village (the same houses, home where it was), the villager's pockets, stats and scorecard, and the villager themself: their name, the pet, friends & family. The game and the wallpaper share it. The desktop app and the web version read the same save files.");
        File f = new File(getFilesDir(), "save.toml");
        note(f.exists() ? "Last saved: " + android.text.format.DateFormat.format("yyyy-MM-dd HH:mm", f.lastModified()) : "Not saved yet (play a while first).");
        LinearLayout row = new LinearLayout(this);
        row.setPadding(0, dp(8), 0, 0);
        row.addView(bigButton("⇩  Export", v -> pickFile(true)), new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        View gap = new View(this);
        row.addView(gap, new LinearLayout.LayoutParams(dp(10), 1));
        row.addView(bigButton("⇧  Import", v -> pickFile(false)), new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        col.addView(row);
        Button fresh = bigButton("✦  A new village", v -> new android.app.AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                .setMessage("A new village? This one's pockets and scorecard go with it.")
                .setPositiveButton("New village", (d, w) -> {
                    new File(getFilesDir(), "save.toml").delete();
                    toast("a new village: it's laid out next time the game opens");
                    showTab(5);
                })
                .setNegativeButton("Keep this one", null)
                .show());
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.topMargin = dp(10);
        col.addView(fresh, lp);
    }

    // ------------------------------------------------------------ the save

    private void pickFile(boolean export) {
        Intent i = new Intent(export ? Intent.ACTION_CREATE_DOCUMENT : Intent.ACTION_OPEN_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        i.setType(export ? "application/toml" : "*/*");
        if (export) {
            i.putExtra(Intent.EXTRA_TITLE, "xmas-tui-save-" + android.text.format.DateFormat.format("yyyy-MM-dd", System.currentTimeMillis()) + ".toml");
        }
        try {
            startActivityForResult(i, export ? EXPORT : IMPORT);
        } catch (ActivityNotFoundException e) {
            toast("no file picker on this phone");
        }
    }

    @Override
    protected void onActivityResult(int req, int res, Intent data) {
        super.onActivityResult(req, res, data);
        if (res != RESULT_OK || data == null || data.getData() == null) return;
        Uri uri = data.getData();
        File f = new File(getFilesDir(), "save.toml");
        try {
            if (req == EXPORT) {
                byte[] b = read(new FileInputStream(f));
                try (OutputStream out = getContentResolver().openOutputStream(uri)) {
                    if (out != null) out.write(b);
                }
                toast("saved a copy");
            } else {
                InputStream in = getContentResolver().openInputStream(uri);
                if (in == null) return;
                String text = new String(read(in), "UTF-8");
                if (Native.saveSeed(text) == 0) {
                    toast("that isn't an xmas-tui save");
                    return;
                }
                try (FileOutputStream out = new FileOutputStream(f)) {
                    out.write(text.getBytes("UTF-8"));
                }
                // the villager too: their name, the pet, friends & family
                Prefs.applyYou(this, Native.youJson(text));
                toast("imported: the village, the villager and their things");
                showTab(tab);
            }
        } catch (Exception e) {
            toast("couldn't: " + e.getMessage());
        }
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

    private void toast(String s) {
        Toast.makeText(this, s, Toast.LENGTH_SHORT).show();
    }

    // ------------------------------------------------------------ pieces

    /** The size setting, or what the older "detail" one comes to. */
    private int sizeNow() {
        int legacy = Math.round(155f * 100f / Math.max(60, prefs.getInt("detail", 155)));
        return Math.max(50, Math.min(200, prefs.getInt("size", legacy)));
    }

    private void setWallpaper() {
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

    private int dp(float v) {
        return Math.round(TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, getResources().getDisplayMetrics()));
    }

    private GradientDrawable rounded(int fill, int edge, int radius) {
        GradientDrawable g = new GradientDrawable();
        g.setColor(fill);
        g.setCornerRadius(dp(radius));
        g.setStroke(dp(1.5f), edge);
        return g;
    }

    private Button bigButton(String s, View.OnClickListener on) {
        Button b = new Button(this);
        b.setText(s);
        b.setAllCaps(false);
        b.setTextColor(Color.rgb(20, 22, 40));
        b.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        b.setBackground(rounded(GOLD, Color.rgb(200, 150, 60), 12));
        b.setOnClickListener(on);
        return b;
    }

    private TextView label(String s, float sp, int color) {
        TextView t = new TextView(this);
        t.setText(s);
        t.setTextSize(sp);
        t.setTextColor(color);
        return t;
    }

    private void note(String s) {
        TextView t = label(s, 13, DIM);
        t.setPadding(0, dp(4), 0, dp(6));
        col.addView(t);
    }

    /** A new card on the page, headed. */
    private void card(String s) {
        col = new LinearLayout(this);
        col.setOrientation(LinearLayout.VERTICAL);
        col.setPadding(dp(14), dp(10), dp(14), dp(12));
        col.setBackground(rounded(PANEL, Color.rgb(120, 100, 50), 14));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.topMargin = dp(12);
        page.addView(col, lp);
        TextView t = label("❄ " + s, 14, GOLD);
        t.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        t.setPadding(0, 0, 0, dp(4));
        col.addView(t);
    }

    private LinearLayout row(String name) {
        LinearLayout r = new LinearLayout(this);
        r.setGravity(Gravity.CENTER_VERTICAL);
        r.setPadding(0, dp(5), 0, dp(5));
        TextView t = label(name, 15, TEXT);
        r.addView(t, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        col.addView(r);
        return r;
    }

    private void toggle(String name, final String key, boolean def) {
        LinearLayout r = row(name);
        Switch s = new Switch(this);
        s.setChecked(prefs.getBoolean(key, def));
        s.setOnCheckedChangeListener((b, on) -> prefs.edit().putBoolean(key, on).apply());
        r.addView(s);
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

    private void text(String name, final String key, String hint, final int max) {
        LinearLayout r = row(name);
        EditText e = new EditText(this);
        e.setSingleLine(true);
        e.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_WORDS);
        e.setHint(hint);
        e.setTextColor(Color.WHITE);
        e.setHintTextColor(Color.rgb(110, 120, 150));
        e.setText(prefs.getString(key, ""));
        e.addTextChangedListener(watcher(v -> prefs.edit().putString(key, v.length() > max ? v.substring(0, max) : v).apply()));
        r.addView(e, new LinearLayout.LayoutParams(dp(170), ViewGroup.LayoutParams.WRAP_CONTENT));
    }

    private Spinner spinner(LinearLayout r, String[] items, int selected) {
        Spinner sp = new Spinner(this);
        ArrayAdapter<String> ad = new ArrayAdapter<>(this, android.R.layout.simple_spinner_item, items);
        ad.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
        sp.setAdapter(ad);
        sp.setSelection(Math.max(0, selected));
        r.addView(sp);
        return sp;
    }

    private void choice(String name, final String key, final String[] items, String def) {
        LinearLayout r = row(name);
        String cur = prefs.getString(key, def);
        int sel = 0;
        for (int i = 0; i < items.length; i++) if (items[i].equals(cur)) sel = i;
        final int first = sel;
        Spinner sp = spinner(r, items, sel);
        sp.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override
            public void onItemSelected(AdapterView<?> parent, View view, int pos, long id) {
                prefs.edit().putString(key, items[pos]).apply();
                // a different pet has different coats
                if (key.equals("pet") && pos != first) showTab(tab);
            }

            @Override
            public void onNothingSelected(AdapterView<?> parent) {}
        });
    }

    /** A small labelled choice (a friend's coat, hat…): "any" stores "". */
    private View miniChoice(String name, final String[] items, String cur, final Got g) {
        LinearLayout r = new LinearLayout(this);
        r.setGravity(Gravity.CENTER_VERTICAL);
        r.addView(label(name, 13, DIM));
        int sel = 0;
        for (int i = 0; i < items.length; i++) if (items[i].equals(cur)) sel = i;
        Spinner sp = spinner(r, items, sel);
        sp.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override
            public void onItemSelected(AdapterView<?> parent, View view, int pos, long id) {
                g.got(pos == 0 ? "" : items[pos]);
            }

            @Override
            public void onNothingSelected(AdapterView<?> parent) {}
        });
        r.setLayoutParams(new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        return r;
    }

    private void choiceInt(String name, final String key, final int[] items, int def) {
        LinearLayout r = row(name);
        int cur = prefs.getInt(key, def);
        String[] labels = new String[items.length];
        int sel = 0;
        for (int i = 0; i < items.length; i++) {
            labels[i] = items[i] + " fps";
            if (items[i] == cur) sel = i;
        }
        Spinner sp = spinner(r, labels, sel);
        sp.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override
            public void onItemSelected(AdapterView<?> parent, View view, int pos, long id) {
                prefs.edit().putInt(key, items[pos]).apply();
            }

            @Override
            public void onNothingSelected(AdapterView<?> parent) {}
        });
    }

    private void slider(String name, final String key, final int min, int max, int def, final String unit) {
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(0, dp(4), 0, dp(4));
        LinearLayout top = new LinearLayout(this);
        TextView t = label(name, 15, TEXT);
        top.addView(t, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        final TextView val = label("", 14, GOLD);
        val.setTypeface(Typeface.MONOSPACE);
        int cur = Math.max(min, Math.min(max, prefs.getInt(key, def)));
        val.setText(cur + unit);
        top.addView(val);
        box.addView(top);
        SeekBar sb = new SeekBar(this);
        sb.setMax(max - min);
        sb.setProgress(cur - min);
        sb.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override
            public void onProgressChanged(SeekBar s, int p, boolean user) {
                val.setText((p + min) + unit);
                if (user) prefs.edit().putInt(key, p + min).apply();
            }

            @Override
            public void onStartTrackingTouch(SeekBar s) {}

            @Override
            public void onStopTrackingTouch(SeekBar s) {}
        });
        box.addView(sb);
        col.addView(box);
    }
}
