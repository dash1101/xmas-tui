package io.github.dash1101.xmastui.core;

/**
 * A small ARGB pixel canvas. The wallpaper is drawn at a low resolution and
 * scaled up with crisp (unfiltered) pixels, like the terminal version's
 * half-block pixels.
 */
public final class Pix {
    public final int w, h;
    public final int[] p;
    /** World column at the left edge (for things that scroll with the village). */
    public int ox;

    public Pix(int w, int h) {
        this.w = w;
        this.h = h;
        this.p = new int[w * h];
    }

    public static int hex(int v) {
        return 0xff000000 | v;
    }

    public static int rgb(float r, float g, float b) {
        return 0xff000000 | (c8(r) << 16) | (c8(g) << 8) | c8(b);
    }

    private static int c8(float v) {
        int i = (int) (v + 0.5f);
        return i < 0 ? 0 : (i > 255 ? 255 : i);
    }

    public static int r(int c) {
        return (c >> 16) & 0xff;
    }

    public static int g(int c) {
        return (c >> 8) & 0xff;
    }

    public static int b(int c) {
        return c & 0xff;
    }

    public static int mix(int a, int b, float t) {
        if (t <= 0f) return a;
        if (t >= 1f) return b;
        return rgb(r(a) + (r(b) - r(a)) * t, g(a) + (g(b) - g(a)) * t, b(a) + (b(b) - b(a)) * t);
    }

    public static int mul(int c, float k) {
        return rgb(r(c) * k, g(c) * k, b(c) * k);
    }

    public static int add(int a, int b, float k) {
        return rgb(r(a) + r(b) * k, g(a) + g(b) * k, b(a) + b(b) * k);
    }

    /** A pixel in world coordinates (the camera is subtracted). */
    public void set(int x, int y, int c) {
        setS(x - ox, y, c);
    }

    /** A pixel in screen coordinates. */
    public void setS(int x, int y, int c) {
        if (x >= 0 && y >= 0 && x < w && y < h) p[y * w + x] = c;
    }

    public int getS(int x, int y) {
        if (x >= 0 && y >= 0 && x < w && y < h) return p[y * w + x];
        return 0xff000000;
    }

    public void blend(int x, int y, int c, float a) {
        blendS(x - ox, y, c, a);
    }

    public void blendS(int x, int y, int c, float a) {
        if (x >= 0 && y >= 0 && x < w && y < h) {
            int i = y * w + x;
            p[i] = mix(p[i], c, a);
        }
    }

    public void rect(int x, int y, int rw, int rh, int c) {
        for (int yy = y; yy < y + rh; yy++) for (int xx = x; xx < x + rw; xx++) set(xx, yy, c);
    }

    public void rectS(int x, int y, int rw, int rh, int c) {
        for (int yy = y; yy < y + rh; yy++) for (int xx = x; xx < x + rw; xx++) setS(xx, yy, c);
    }

    /** A soft additive glow around (cx, cy) (world x), radius r. */
    public void glow(float cx, float cy, float r, int c, float k) {
        glowS(cx - ox, cy, r, c, k);
    }

    public void glowS(float cx, float cy, float r, int c, float k) {
        int x0 = (int) Math.floor(cx - r), x1 = (int) Math.ceil(cx + r);
        int y0 = (int) Math.floor(cy - r), y1 = (int) Math.ceil(cy + r);
        float r2 = r * r;
        for (int y = Math.max(0, y0); y <= Math.min(h - 1, y1); y++) {
            for (int x = Math.max(0, x0); x <= Math.min(w - 1, x1); x++) {
                float dx = x + 0.5f - cx, dy = y + 0.5f - cy;
                float d2 = dx * dx + dy * dy;
                if (d2 < r2) {
                    float a = 1f - d2 / r2;
                    int i = y * w + x;
                    p[i] = add(p[i], c, a * a * k);
                }
            }
        }
    }

    /** A filled disc (world x), soft at the edge. */
    public void disc(float cx, float cy, float r, int c, float a) {
        int x0 = (int) Math.floor(cx - r - 1), x1 = (int) Math.ceil(cx + r + 1);
        int y0 = (int) Math.floor(cy - r - 1), y1 = (int) Math.ceil(cy + r + 1);
        for (int y = y0; y <= y1; y++) {
            for (int x = x0; x <= x1; x++) {
                float dx = x + 0.5f - cx, dy = y + 0.5f - cy;
                float d = (float) Math.sqrt(dx * dx + dy * dy);
                float cov = Math.max(0f, Math.min(1f, r + 0.5f - d));
                if (cov > 0f) blend(x, y, c, a * cov);
            }
        }
    }
}
