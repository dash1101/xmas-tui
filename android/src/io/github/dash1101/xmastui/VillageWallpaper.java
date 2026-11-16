package io.github.dash1101.xmastui;

import android.app.WallpaperManager;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.Rect;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.service.wallpaper.WallpaperService;
import android.view.SurfaceHolder;

import io.github.dash1101.xmastui.core.Bubbles;
import io.github.dash1101.xmastui.core.Config;
import io.github.dash1101.xmastui.core.Pix;
import io.github.dash1101.xmastui.core.Village;

/**
 * The live wallpaper: the village drawn at a low resolution and scaled up
 * with crisp pixels, speech bubbles on top as text. It runs only while
 * it's showing, at the frame rate set in the settings (24 fps by default).
 */
public class VillageWallpaper extends WallpaperService {
    @Override
    public Engine onCreateEngine() {
        return new VillageEngine();
    }

    final class VillageEngine extends Engine implements SharedPreferences.OnSharedPreferenceChangeListener {
        private final Handler handler = new Handler(Looper.getMainLooper());
        private final Runnable tick = this::frame;
        private Config cfg;
        private Village village;
        private Pix pix;
        private Bitmap bmp;
        private int scale = 6, sw, sh;
        private boolean visible;
        private long last;
        private float pan = 0.5f;
        private final Paint pixels = new Paint();
        private final Paint text = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint bubble = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Rect dst = new Rect();
        private final RectF box = new RectF();
        private final Path tail = new Path();

        @Override
        public void onCreate(SurfaceHolder holder) {
            super.onCreate(holder);
            cfg = Prefs.load(VillageWallpaper.this);
            Prefs.get(VillageWallpaper.this).registerOnSharedPreferenceChangeListener(this);
            pixels.setFilterBitmap(false);
            pixels.setDither(false);
            text.setTypeface(Typeface.MONOSPACE);
            text.setColor(Color.rgb(42, 26, 20));
            bubble.setColor(Color.rgb(244, 239, 230));
        }

        @Override
        public void onDestroy() {
            handler.removeCallbacks(tick);
            Prefs.get(VillageWallpaper.this).unregisterOnSharedPreferenceChangeListener(this);
            super.onDestroy();
        }

        @Override
        public void onSurfaceChanged(SurfaceHolder holder, int format, int width, int height) {
            super.onSurfaceChanged(holder, format, width, height);
            sw = width;
            sh = height;
            build(false);
            frame();
        }

        /** (Re)make the village for the screen; `fresh`: even if the size is the same. */
        private void build(boolean fresh) {
            if (sw <= 0 || sh <= 0) return;
            // wallpaper pixels across the screen's short side (the "size"
            // setting), so turning a phone or tablet sideways shows more of
            // the village at the same size instead of zooming it in
            int across = Math.max(60, Math.min(340, cfg.detail));
            int s = Math.max(2, Math.round(Math.min(sw, sh) / (float) across));
            int w = (sw + s - 1) / s, h = (sh + s - 1) / s;
            if (!fresh && village != null && pix != null && pix.w == w && pix.h == h) return;
            scale = s;
            village = new Village(w, h, cfg, SystemClock.elapsedRealtime());
            village.setPan(pan);
            pix = new Pix(w, h);
            bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888);
            dst.set(0, 0, w * s, h * s);
            text.setTextSize(Math.max(18f, Math.min(sw, sh) / 34f));
            last = 0;
        }

        @Override
        public void onVisibilityChanged(boolean v) {
            visible = v;
            handler.removeCallbacks(tick);
            if (v) {
                last = 0;
                frame();
            }
        }

        @Override
        public void onSurfaceDestroyed(SurfaceHolder holder) {
            visible = false;
            handler.removeCallbacks(tick);
            super.onSurfaceDestroyed(holder);
        }

        @Override
        public void onOffsetsChanged(float xOffset, float yOffset, float xStep, float yStep, int xPixels, int yPixels) {
            pan = xOffset;
            if (village != null) village.setPan(xOffset);
        }

        @Override
        public Bundle onCommand(String action, int x, int y, int z, Bundle extras, boolean resultRequested) {
            if (WallpaperManager.COMMAND_TAP.equals(action) && village != null) {
                village.tap(x / (float) scale, y / (float) scale);
            }
            return null;
        }

        @Override
        public void onSharedPreferenceChanged(SharedPreferences p, String key) {
            Config old = cfg;
            cfg = Prefs.load(VillageWallpaper.this);
            if (village == null) return;
            if (old.detail != cfg.detail) build(true);
            else village.setConfig(cfg);
        }

        private void frame() {
            handler.removeCallbacks(tick);
            if (village == null) return;
            long now = SystemClock.uptimeMillis();
            float dt = last == 0 ? 1f / cfg.fps : (now - last) / 1000f;
            last = now;
            village.update(dt);
            village.render(pix);
            bmp.setPixels(pix.p, 0, pix.w, 0, 0, pix.w, pix.h);
            SurfaceHolder holder = getSurfaceHolder();
            Canvas c = null;
            try {
                c = lock(holder);
                if (c != null) {
                    c.drawBitmap(bmp, null, dst, pixels);
                    drawBubbles(c);
                }
            } catch (RuntimeException e) {
                // the surface went away mid-frame: try again next time
            } finally {
                if (c != null) {
                    try {
                        holder.unlockCanvasAndPost(c);
                    } catch (RuntimeException ignored) {
                    }
                }
            }
            if (visible) {
                long frameMs = 1000L / Math.max(1, cfg.fps);
                long spent = SystemClock.uptimeMillis() - now;
                handler.postDelayed(tick, Math.max(1L, frameMs - spent));
            }
        }

        private Canvas lock(SurfaceHolder holder) {
            if (Build.VERSION.SDK_INT >= 26) {
                try {
                    return holder.lockHardwareCanvas();
                } catch (RuntimeException e) {
                    // fall back to a software canvas
                }
            }
            return holder.lockCanvas();
        }

        /** Speech bubbles: rounded boxes with a tail, the text in a monospace font. */
        private void drawBubbles(Canvas c) {
            float size = text.getTextSize();
            Paint.FontMetrics fm = text.getFontMetrics();
            float lh = fm.descent - fm.ascent;
            float pad = size * 0.45f;
            for (Village.Say s : village.bubbles()) {
                String[] lines = Bubbles.wrap(s.text, 20);
                float tw = 0;
                for (String l : lines) tw = Math.max(tw, text.measureText(l));
                float bw = tw + pad * 2, bh = lh * lines.length + pad;
                float cx = s.x * scale, by = s.y * scale - size * 0.6f;
                float x0 = Math.max(6, Math.min(sw - bw - 6, cx - bw / 2)), y0 = by - bh;
                int a = Math.round(235 * s.alpha);
                bubble.setAlpha(a);
                box.set(x0, y0, x0 + bw, y0 + bh);
                c.drawRoundRect(box, pad, pad, bubble);
                tail.reset();
                tail.moveTo(cx - pad / 2, y0 + bh - 1);
                tail.lineTo(cx + pad / 2, y0 + bh - 1);
                tail.lineTo(cx, y0 + bh + pad);
                tail.close();
                c.drawPath(tail, bubble);
                text.setAlpha(Math.round(255 * s.alpha));
                for (int k = 0; k < lines.length; k++) c.drawText(lines[k], x0 + pad, y0 + pad / 2 - fm.ascent + k * lh, text);
            }
        }
    }
}
