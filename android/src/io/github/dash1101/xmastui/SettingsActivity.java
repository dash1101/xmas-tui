package io.github.dash1101.xmastui;

import android.app.Activity;
import android.app.WallpaperManager;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Intent;
import android.os.Bundle;

/**
 * The settings on a page of their own: what the wallpaper picker's
 * "Settings" opens. The same panel the game shows over the village (see
 * `SettingsPanel`), with ▶ play and ❄ wallpaper in its title bar.
 */
public class SettingsActivity extends Activity implements SettingsPanel.Host {
    private SettingsPanel panel;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        panel = new SettingsPanel(this, this, false);
        setContentView(panel.view());
    }

    @Override
    public void changed() {
        // (kept in the settings: the game takes them in when it's back, the
        // wallpaper when it next shows)
    }

    @Override
    public void close() {
        finish();
    }

    @Override
    public void wallpaper() {
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

    @Override
    public void freshVillage() {
        SettingsPanel.toast(this, "a new village: it's laid out next time the game opens");
    }

    @Override
    protected void onActivityResult(int req, int res, Intent data) {
        if (!SettingsPanel.onResult(this, req, res, data, () -> panel.build())) {
            super.onActivityResult(req, res, data);
        }
    }
}
