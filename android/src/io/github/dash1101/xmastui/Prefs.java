package io.github.dash1101.xmastui;

import android.content.Context;
import android.content.SharedPreferences;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.List;

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
     * wallpaper's), as TOML in the desktop config file's shape; the save
     * file (the same village, and the villager's things, in both) goes
     * beside the app's other files. On the wallpaper the villager's always
     * on autopilot, and it's quiet.
     */
    /** The settings the game's menu keeps (the desktop's own config), once it has. */
    static java.io.File configFile(Context c) {
        return new java.io.File(c.getFilesDir(), "config.toml");
    }

    /** The game's menu changed the settings: kept for the game and the wallpaper. */
    static void putConfig(Context c, String toml) {
        java.io.File f = configFile(c), tmp = new java.io.File(c.getFilesDir(), "config.toml.tmp");
        try (java.io.FileOutputStream out = new java.io.FileOutputStream(tmp)) {
            out.write(toml.getBytes("UTF-8"));
        } catch (Exception e) {
            return;
        }
        tmp.renameTo(f);
    }

    static String readConfig(Context c) {
        java.io.File f = configFile(c);
        if (!f.exists()) return null;
        try {
            byte[] b = new byte[(int) f.length()];
            try (java.io.FileInputStream in = new java.io.FileInputStream(f)) {
                int n = 0;
                while (n < b.length) {
                    int r = in.read(b, n, b.length - n);
                    if (r < 0) break;
                    n += r;
                }
            }
            return new String(b, "UTF-8");
        } catch (Exception e) {
            return null;
        }
    }

    /** The settings for the game (`wallpaper`: for the live wallpaper: on autopilot, quiet). */
    static String toml(Context c, boolean wallpaper) {
        String kept = readConfig(c);
        if (kept != null) {
            if (!wallpaper) return kept;
            // the villager off on autopilot, no sound
            String s = kept.replaceFirst("(?m)^autopilot = (true|false)$", "autopilot = true");
            int snd = s.indexOf("\n[sound]");
            if (snd >= 0) {
                String head = s.substring(0, snd), tail = s.substring(snd);
                s = head + tail.replaceFirst("(?m)^enabled = (true|false)$", "enabled = false");
            }
            return s;
        }
        return fromPrefs(c, wallpaper);
    }

    /** The settings from the phone's own preferences (before the game's menu kept its own). */
    static String fromPrefs(Context c, boolean wallpaper) {
        SharedPreferences p = get(c);
        String time = p.getString("time", "auto");
        if (time.equals("day")) {
            time = "noon";
        }
        StringBuilder s = new StringBuilder();
        s.append("save_path = ").append(q(new java.io.File(c.getFilesDir(), "save.toml").getAbsolutePath())).append('\n');
        s.append("zoom = ").append(p.getInt("zoom", 100) / 100f).append('\n');
        // (online is the game's: the wallpaper stays off it)
        s.append("online = ").append(!wallpaper).append('\n');
        s.append("[scene]\n");
        s.append("name = ").append(q(p.getString("name", ""))).append('\n');
        s.append("time = ").append(q(time)).append('\n');
        s.append("snow = ").append(p.getInt("snow", 100) / 100f).append('\n');
        s.append("wind = ").append(p.getInt("wind", 100) / 100f).append('\n');
        s.append("aurora = ").append(p.getBoolean("aurora", true)).append('\n');
        s.append("glow = ").append(p.getInt("glow", 100) / 100f).append('\n');
        s.append("events = ").append(p.getInt("events", 100) / 100f).append('\n');
        s.append("critters = ").append(p.getInt("critters", 100) / 100f).append('\n');
        s.append("most_at_once = ").append(p.getInt("most_at_once", 24)).append('\n');
        s.append("bushes = ").append(p.getBoolean("bushes", true)).append('\n');
        s.append("sleigh_minutes = ").append(p.getInt("sleigh_minutes", 4)).append('\n');
        s.append("talk_time = ").append(p.getInt("talk_time", 100) / 100f).append('\n');
        s.append("chattiness = ").append(p.getInt("chattiness", 100) / 100f).append('\n');
        s.append("walk_speed = ").append(p.getInt("walk_speed", 100) / 100f).append('\n');
        s.append("only_friends = ").append(p.getBoolean("only_friends", false)).append('\n');
        s.append("autopilot = ").append(wallpaper || p.getBoolean("autopilot", false)).append('\n');
        s.append("village_limit = ").append(Math.max(2, Math.min(48, p.getInt("village_limit", 8)))).append('\n');
        s.append("clean_words = ").append(p.getBoolean("clean_words", true)).append('\n');
        // (the wallpaper: on autopilot from the start, anywhere)
        if (!wallpaper) {
            s.append("autopilot_after = ").append(num(p.getString("auto_after", "20"), 20f)).append('\n');
            s.append("autopilot_area = ").append(q(p.getString("auto_area", "anywhere"))).append('\n');
            s.append("autopilot_reach = ").append(num(p.getString("auto_reach", "1"), 1f)).append('\n');
        }
        s.append("motto = ").append(q(p.getString("motto", ""))).append('\n');
        s.append("[scene.wear]\n");
        for (String part : new String[] {"coat", "hat", "hair", "skin"}) {
            String v = p.getString("wear_" + part, "classic");
            s.append(part).append(" = ").append(q("classic".equals(v) ? "" : v)).append('\n');
        }
        s.append("scarf = ").append(p.getBoolean("wear_scarf", true)).append('\n');
        s.append("[scene.pet]\n");
        s.append("kind = ").append(q(p.getString("pet", "cat"))).append('\n');
        s.append("coat = ").append(q(p.getString("pet_coat", ""))).append('\n');
        s.append("name = ").append(q(p.getString("pet_name", ""))).append('\n');
        for (Friend f : friends(c)) {
            s.append("[[scene.friends]]\n");
            s.append("name = ").append(q(f.name)).append('\n');
            s.append("kid = ").append(f.kid).append('\n');
            s.append("coat = ").append(q(f.coat)).append('\n');
            s.append("hat = ").append(q(f.hat)).append('\n');
            s.append("hair = ").append(q(f.hair)).append('\n');
            s.append("skin = ").append(q(f.skin)).append('\n');
            s.append("scarf = ").append(f.scarf).append('\n');
        }
        s.append("[ui]\n");
        s.append("look = ").append(q(p.getString("look", "classic"))).append('\n');
        s.append("scale = ").append(Math.max(100, Math.min(250, Math.round(p.getInt("ui_scale", 140) / 1.4f))) / 100f).append('\n');
        s.append("[sound]\n");
        s.append("enabled = ").append(!wallpaper && p.getBoolean("game_sound", true)).append('\n');
        s.append("master = ").append(p.getInt("vol_master", 50) / 100f).append('\n');
        s.append("music = ").append(p.getInt("vol_music", 60) / 100f).append('\n');
        s.append("sfx = ").append(p.getInt("vol_sfx", 70) / 100f).append('\n');
        s.append("ambience = ").append(p.getInt("vol_ambience", 50) / 100f).append('\n');
        s.append("track = ").append(q(p.getString("track", "shuffle"))).append('\n');
        return s.toString();
    }

    /** A number from a stored choice (or `def`, if it isn't one). */
    static float num(String v, float def) {
        try {
            float f = Float.parseFloat(v);
            return Float.isNaN(f) || Float.isInfinite(f) ? def : f;
        } catch (NumberFormatException e) {
            return def;
        }
    }

    static String q(String v) {
        return "\"" + v.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", " ") + "\"";
    }

    /** Someone in the friends & family list (as in the desktop's config). */
    static final class Friend {
        String name = "", coat = "", hat = "", hair = "", skin = "";
        boolean kid, scarf = true;

        JSONObject json() {
            JSONObject o = new JSONObject();
            try {
                o.put("name", name).put("kid", kid).put("coat", coat).put("hat", hat).put("hair", hair).put("skin", skin).put("scarf", scarf);
            } catch (Exception ignored) {
            }
            return o;
        }

        static Friend of(JSONObject o) {
            Friend f = new Friend();
            f.name = o.optString("name", "");
            f.kid = o.optBoolean("kid", false);
            f.coat = o.optString("coat", "");
            f.hat = o.optString("hat", "");
            f.hair = o.optString("hair", "");
            f.skin = o.optString("skin", "");
            f.scarf = o.optBoolean("scarf", true);
            return f;
        }
    }

    static List<Friend> friends(Context c) {
        List<Friend> out = new ArrayList<>();
        try {
            JSONArray a = new JSONArray(get(c).getString("friends", "[]"));
            for (int i = 0; i < a.length(); i++) {
                Friend f = Friend.of(a.getJSONObject(i));
                if (!f.name.trim().isEmpty()) out.add(f);
            }
        } catch (Exception ignored) {
        }
        return out;
    }

    static void putFriends(Context c, List<Friend> fs) {
        JSONArray a = new JSONArray();
        for (Friend f : fs) a.put(f.json());
        get(c).edit().putString("friends", a.toString()).apply();
    }

    /**
     * A save's villager (from Native.youJson: name, pet, friends & family,
     * the panels' look) into the settings: an imported save brings them.
     */
    static void applyYou(Context c, String json) {
        if (json == null || json.isEmpty()) return;
        try {
            JSONObject o = new JSONObject(json);
            SharedPreferences.Editor e = get(c).edit();
            e.putString("name", o.optString("name", ""));
            JSONObject pet = o.optJSONObject("pet");
            if (pet != null) {
                e.putString("pet", pet.optString("kind", "cat"));
                e.putString("pet_coat", pet.optString("coat", ""));
                e.putString("pet_name", pet.optString("name", ""));
            }
            JSONArray fs = o.optJSONArray("friends");
            if (fs != null) e.putString("friends", fs.toString());
            e.putBoolean("only_friends", o.optBoolean("only_friends", false));
            String look = o.optString("look", "");
            if (!look.isEmpty()) e.putString("look", look);
            e.putString("motto", o.optString("motto", ""));
            JSONObject wear = o.optJSONObject("wear");
            if (wear != null) {
                for (String part : new String[] {"coat", "hat", "hair", "skin"}) {
                    String v = wear.optString(part, "");
                    e.putString("wear_" + part, v.isEmpty() ? "classic" : v);
                }
                e.putBoolean("wear_scarf", wear.optBoolean("scarf", true));
            }
            e.apply();
        } catch (Exception ignored) {
        }
    }

    /** The wallpaper is the classic one (the snowy dock), or the game's own village on autopilot. */
    /** Every setting back to how it starts, keeping the villager's own: name, motto, look, pet, friends & family (the village is in the save). */
    static void resetKeepingOwn(Context c) {
        SharedPreferences p = get(c);
        SharedPreferences.Editor e = p.edit();
        for (String k : p.getAll().keySet()) {
            boolean own = k.equals("name") || k.equals("motto") || k.startsWith("wear_") || k.startsWith("pet") || k.equals("friends") || k.equals("game_seen");
            if (!own) e.remove(k);
        }
        e.apply();
    }

    static boolean nativeWallpaper(Context c) {
        return get(c).getString("wall_style", "classic").equals("village");
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
        // people: how many at most, friends & family among them (or only them)
        cfg.mostPeople = p.getInt("classic_people", 0);
        cfg.onlyFriends = p.getBoolean("only_friends", false);
        for (Friend f : friends(c)) cfg.friends.add(new Config.Friend(f.name.trim(), f.kid, f.coat, f.hat));
        // size (percent; 100 = 155 pixels across the short side), or the
        // older "detail" setting (pixels across) from before there was one
        int size = p.getInt("size", Math.round(155f * 100f / Math.max(60, p.getInt("detail", 155))));
        cfg.detail = Math.round(155f * 100f / Math.max(40, Math.min(250, size)));
        return cfg;
    }
}
