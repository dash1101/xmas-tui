package io.github.dash1101.xmastui;

import android.app.Activity;
import android.app.WallpaperManager;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Color;
import android.graphics.Typeface;
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
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.Spinner;
import android.widget.Switch;
import android.widget.TextView;

/** The wallpaper's settings (also the app's own screen, with a button to set it). */
public class SettingsActivity extends Activity {
    private SharedPreferences prefs;
    private LinearLayout col;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        prefs = Prefs.get(this);
        ScrollView scroll = new ScrollView(this);
        scroll.setBackgroundColor(Color.rgb(12, 16, 36));

        col = new LinearLayout(this);
        col.setOrientation(LinearLayout.VERTICAL);
        int pad = dp(20);
        col.setPadding(pad, pad, pad, pad + dp(56));
        // (Android 15 draws apps edge to edge: keep clear of the system bars)
        scroll.setOnApplyWindowInsetsListener((v, insets) -> {
            @SuppressWarnings("deprecation")
            int top = insets.getSystemWindowInsetTop(), bottom = insets.getSystemWindowInsetBottom();
            col.setPadding(pad, pad + top, pad, pad + dp(24) + bottom);
            return insets;
        });
        // (so the name field doesn't grab focus and scroll the page down)
        col.setFocusableInTouchMode(true);
        col.requestFocus();
        scroll.addView(col);
        setContentView(scroll);

        LinearLayout head = new LinearLayout(this);
        head.setGravity(Gravity.CENTER_VERTICAL);
        ImageView icon = new ImageView(this);
        icon.setImageResource(R.mipmap.ic_launcher);
        head.addView(icon, new LinearLayout.LayoutParams(dp(48), dp(48)));
        TextView title = label("xmas-tui village", 22, Color.rgb(255, 211, 107));
        title.setPadding(dp(12), 0, 0, 0);
        title.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        head.addView(title);
        col.addView(head);
        col.addView(note("A snowy village on the home screen. Folk go about their evening on their own; tap someone to say hi, tap the sky for a shooting star, tap the dock to knock the snow off."));

        Button play = new Button(this);
        play.setText("▶  Play in the village");
        play.setOnClickListener(v -> startActivity(new Intent(this, GameActivity.class)));
        LinearLayout.LayoutParams pl = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        pl.topMargin = dp(12);
        col.addView(play, pl);
        col.addView(note("Walk the villager about the whole village, as on the desktop: talk to folk, climb roofs, build snowmen with the kids, visit the houses."));

        Button set = new Button(this);
        set.setText("Set as live wallpaper");
        set.setOnClickListener(v -> setWallpaper());
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.topMargin = dp(12);
        col.addView(set, lp);

        section("THE VILLAGE");
        text("Your villager's name", "name", "neighbour");
        choice("Pet", "pet", new String[] {"cat", "dog", "bunny", "none"}, "cat");
        text("Pet's name", "pet_name", "Mittens");
        toggle("Speech bubbles", "bubbles", true);
        choice("Time of day", "time", new String[] {"auto", "night", "dusk", "day", "dawn"}, "auto");
        slider("Snowfall", "snow", 0, 200, 100, "%");
        slider("Size", "size", 50, 200, sizeNow(), "%");
        col.addView(note("Bigger or smaller village. It stays the same size whichever way the screen is turned: sideways just shows more of it."));

        section("FRAME RATE");
        choiceInt("Frames per second", "fps", new int[] {10, 15, 20, 24, 30, 45, 60}, 24);
        col.addView(note("Lower saves battery. It only runs while the home screen is showing."));

        section("PLAYING IN THE VILLAGE");
        toggle("Sound", "game_sound", true);
        toggle("Autopilot", "autopilot", false);
        col.addView(note("Left alone a while, the villager goes about their own day: snowmen and igloos, snowballs, feeding the animals, chats, tag with the kids. Touch the joystick or a button to take over."));
        choiceInt("Frames per second", "game_fps", new int[] {20, 24, 30, 45, 60}, 30);

        section("SNOWY DOCK");
        toggle("Show a shelf under the dock", "dock", true);
        choice("Style", "dock_style", new String[] {"frost", "wood", "candy", "stone"}, "frost");
        slider("Top edge (from the bottom)", "dock_lift", 3, 45, 15, "%");
        slider("Height", "dock_height", 2, 25, 9, "%");
        slider("Width", "dock_width", 30, 100, 92, "%");
        slider("Snow pile", "dock_pile", 0, 14, 6, "px");
        col.addView(note("Every launcher puts its dock somewhere different: slide the shelf to sit under yours. Snow lands on it and piles up; tap it to knock the snow off."));
    }

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

    private TextView label(String s, float sp, int color) {
        TextView t = new TextView(this);
        t.setText(s);
        t.setTextSize(sp);
        t.setTextColor(color);
        return t;
    }

    private TextView note(String s) {
        TextView t = label(s, 13, Color.rgb(150, 160, 190));
        t.setPadding(0, dp(6), 0, dp(6));
        return t;
    }

    private void section(String s) {
        TextView t = label(s, 14, Color.rgb(255, 211, 107));
        t.setTypeface(Typeface.MONOSPACE, Typeface.BOLD);
        t.setPadding(0, dp(22), 0, dp(6));
        col.addView(t);
    }

    private LinearLayout row(String name) {
        LinearLayout r = new LinearLayout(this);
        r.setGravity(Gravity.CENTER_VERTICAL);
        r.setPadding(0, dp(6), 0, dp(6));
        TextView t = label(name, 16, Color.rgb(232, 232, 240));
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

    private void text(String name, final String key, String hint) {
        LinearLayout r = row(name);
        EditText e = new EditText(this);
        e.setSingleLine(true);
        e.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_WORDS);
        e.setHint(hint);
        e.setTextColor(Color.WHITE);
        e.setHintTextColor(Color.rgb(110, 120, 150));
        e.setText(prefs.getString(key, ""));
        e.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int a, int b, int c) {}

            @Override
            public void onTextChanged(CharSequence s, int a, int b, int c) {}

            @Override
            public void afterTextChanged(Editable s) {
                String v = s.toString().trim();
                if (v.length() > 16) v = v.substring(0, 16);
                prefs.edit().putString(key, v).apply();
            }
        });
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
        Spinner sp = spinner(r, items, sel);
        sp.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override
            public void onItemSelected(AdapterView<?> parent, View view, int pos, long id) {
                prefs.edit().putString(key, items[pos]).apply();
            }

            @Override
            public void onNothingSelected(AdapterView<?> parent) {}
        });
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
        LinearLayout r = row(name);
        final TextView val = label("", 14, Color.rgb(255, 211, 107));
        val.setTypeface(Typeface.MONOSPACE);
        val.setMinWidth(dp(56));
        val.setGravity(Gravity.END);
        int cur = prefs.getInt(key, def);
        val.setText(cur + unit);
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
        r.addView(sb, new LinearLayout.LayoutParams(dp(150), ViewGroup.LayoutParams.WRAP_CONTENT));
        r.addView(val);
    }
}
