package io.github.dash1101.xmastui;

import android.content.Context;
import android.content.SharedPreferences;

import io.github.dash1101.xmastui.core.Config;

/** The settings, kept in shared preferences. */
final class Prefs {
    static final String FILE = "village";

    private Prefs() {}

    static SharedPreferences get(Context c) {
        return c.getSharedPreferences(FILE, Context.MODE_PRIVATE);
    }

    static Config load(Context c) {
        SharedPreferences p = get(c);
        Config cfg = new Config();
        cfg.fps = p.getInt("fps", 24);
        cfg.snow = p.getInt("snow", 100);
        cfg.dock = p.getBoolean("dock", true);
        cfg.dockLift = p.getInt("dock_lift", 15);
        cfg.dockHeight = p.getInt("dock_height", 9);
        cfg.dockWidth = p.getInt("dock_width", 92);
        cfg.dockStyle = p.getString("dock_style", "frost");
        cfg.dockPile = p.getInt("dock_pile", 6);
        cfg.name = p.getString("name", "");
        cfg.pet = p.getString("pet", "cat");
        cfg.petName = p.getString("pet_name", "");
        cfg.bubbles = p.getBoolean("bubbles", true);
        cfg.time = p.getString("time", "auto");
        cfg.detail = p.getInt("detail", 155);
        return cfg;
    }
}
