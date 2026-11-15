package io.github.dash1101.xmastui.core;

/** A small, fast random source (xorshift), seeded so a village is repeatable. */
public final class Rnd {
    private long s;

    public Rnd(long seed) {
        s = seed == 0 ? 0x9E3779B97F4A7C15L : seed;
    }

    public long next() {
        s ^= s << 13;
        s ^= s >>> 7;
        s ^= s << 17;
        return s;
    }

    /** 0..1 */
    public float f() {
        return (next() >>> 40) / (float) (1L << 24);
    }

    public float range(float a, float b) {
        return a + (b - a) * f();
    }

    /** a..b-1 */
    public int i(int a, int b) {
        return b <= a ? a : a + (int) ((next() >>> 33) % (b - a));
    }

    public boolean chance(float p) {
        return f() < p;
    }

    public <T> T pick(T[] a) {
        return a[i(0, a.length)];
    }

    /** A steady 0..1 value for an integer (texture noise). */
    public static float h01(int x) {
        int h = x * 0x27d4eb2d;
        h ^= h >>> 15;
        h *= 0x85ebca6b;
        h ^= h >>> 13;
        return (h & 0xffffff) / (float) 0x1000000;
    }

    /** Smooth value noise along a line. */
    public static float noise(float x, int seed) {
        int i = (int) Math.floor(x);
        float t = x - i;
        t = t * t * (3 - 2 * t);
        float a = h01(i * 7919 + seed * 104729), b = h01((i + 1) * 7919 + seed * 104729);
        return a + (b - a) * t;
    }
}
