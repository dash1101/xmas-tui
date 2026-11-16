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

    /**
     * The settings the native village takes (the game screen's, and the
     * wallpaper's), as TOML; the save file (the same village, and the
     * villager's things, in both) goes beside the app's other files. On the
     * wallpaper the villager's always on autopilot, and it's quiet.
     */
    static String toml(Context c, boolean wallpaper) {
        SharedPreferences p = get(c);
        String time = p.getString("time", "auto");
        if (time.equals("day")) {
            time = "noon";
        }
        StringBuilder s = new StringBuilder();
        s.append("save_path = ").append(q(new java.io.File(c.getFilesDir(), "save.toml").getAbsolutePath())).append('\n');
        s.append("[scene]\n");
        s.append("name = ").append(q(p.getString("name", ""))).append('\n');
        s.append("time = ").append(q(time)).append('\n');
        s.append("snow = ").append(p.getInt("snow", 100) / 100f).append('\n');
        s.append("autopilot = ").append(wallpaper || p.getBoolean("autopilot", false)).append('\n');
        s.append("[scene.pet]\n");
        s.append("kind = ").append(q(p.getString("pet", "cat"))).append('\n');
        s.append("name = ").append(q(p.getString("pet_name", ""))).append('\n');
        s.append("[sound]\n");
        s.append("enabled = ").append(!wallpaper && p.getBoolean("game_sound", true)).append('\n');
        return s.toString();
    }

    static String q(String v) {
        return "\"" + v.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    /** The wallpaper is the game's own village, on autopilot (or the classic one, with the snowy dock). */
    static boolean nativeWallpaper(Context c) {
        return !get(c).getString("wall_style", "village").equals("classic");
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
        // size (percent; 100 = 155 pixels across the short side), or the
        // older "detail" setting (pixels across) from before there was one
        int size = p.getInt("size", Math.round(155f * 100f / Math.max(60, p.getInt("detail", 155))));
        cfg.detail = Math.round(155f * 100f / Math.max(40, Math.min(250, size)));
        return cfg;
    }
}
