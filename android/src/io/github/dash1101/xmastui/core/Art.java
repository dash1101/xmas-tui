package io.github.dash1101.xmastui.core;

/** Pixel art: villagers (the same folk as the terminal version's homes), pets and animals. */
final class Art {
    private Art() {}

    // people, right-facing, 8 wide; w pompom, h hat/hair, s skin, e eye, r scarf, c coat, b boots
    static final String[] YOU = {
        "   w    ", "   hh   ", "  hhh   ", "  hhhh  ", "  ssss  ", "  sses  ", "  ssss  ", " rrrrr  ", "rr cccc ", "  cccc  ", "  ccccc ", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
    };
    static final String[] HATTED = {
        "   w    ", "   hh   ", "  hhh   ", "  hhhh  ", "  ssss  ", "  sses  ", "  ssss  ", "   ss   ", "  cccc  ", " cccccc ", " cccccc ", "  cccc s", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
    };
    static final String[] ADULT = {
        "        ", "  hhhh  ", " hhhhh  ", " hssss  ", "  sses  ", "  ssss  ", "   ss   ", "  cccc  ", " cccccc ", " cccccc ", "  cccc s", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
    };
    static final String[] ARMS_UP = {
        "        ", "  hhhh  ", " hhhhh  ", " hssss  ", "  sses  ", "  ssss  ", "   ss  s", "  cccccs", " ccccc  ", " cccccc ", "  cccc  ", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
    };
    static final String[] KID = {
        "  hhh   ", " hhhhh  ", " hssss  ", "  sses  ", "  ssss  ", "  cccc  ", " cccccc ", "  cccc s", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
    };
    static final String[] KID_UP = {
        "  hhh   ", " hhhhh  ", " hssss  ", "  sses s", "  ssss s", "  cccccs", " cccccc ", "  cccc  ", "  cccc  ", "  b  b  ", "  b  b  ", " bb bb  ",
    };

    // pets (from the terminal version): b body, m markings, d dark, w light; walk, walk, sit, sleep
    static final String[][] DOG = {
        {"    bm ", "m   bbd", " bbbbk ", " bbbbb ", " b  b  "},
        {"    bm ", " m  bbd", " bbbbk ", " bbbbb ", "  b  b "},
        {"   bm  ", "   bbd ", "m  bk  ", "mbbbb  ", " bb b  "},
        {"       ", "       ", "    bm ", "mbbbbbd", " bbbbb "},
    };
    static final String[][] CAT = {
        {"    m m", "m   bbb", "m   bdb", " bbbbb ", " b   b "},
        {"    m m", " m  bbb", "m   bdb", " bbbbb ", "  b b  "},
        {"   m m ", "   bbb ", "m  bdb ", "m bbbb ", " bb bb "},
        {"       ", "       ", "    m m", " mbbbbb", "mbbbbbd"},
    };
    static final String[][] BUNNY = {
        {"   mm ", "   bb ", "  bbbd", "wbbbb ", " b  b "},
        {"   mm ", "   bb ", " bbbbd", "wbbbb ", "  bb  "},
        {"  mm  ", "  bb  ", "  bbd ", " bbbb ", "wbbbb "},
        {"      ", "      ", "   mm ", " bbbbd", "wbbbbb"},
    };
    // a deer: a antler, b body, l light belly, d nose, w tail, k hoof; walk, walk, graze
    static final String[][] DEER = {
        {"       a a  ", "        a   ", "       bbb  ", "       bbbd ", " bbbbbbbbb  ", "wbbbbbbbbb  ", " lllllllb   ", " b b   b b  ", " b b   b b  ", " k k   k k  "},
        {"       a a  ", "        a   ", "       bbb  ", "       bbbd ", " bbbbbbbbb  ", "wbbbbbbbbb  ", " lllllllb   ", "  b b  b  b ", " b  b   bb  ", " k  k   kk  "},
        {"            ", "            ", "            ", "        a a ", " bbbbbbb a  ", "wbbbbbbbbb  ", " llllllbbbb ", " b b   b bbd", " b b   b b  ", " k k   k k  "},
    };

    /** Draw rows (right-facing; mirrored for dir < 0) with the bottom row on pixel row `feet`. */
    static void draw(Pix f, String[] rows, float cx, int feet, float dir, int[] pal) {
        int n = rows.length;
        int w = 0;
        for (String r : rows) w = Math.max(w, r.length());
        int x0 = Math.round(cx - w / 2f);
        for (int ry = 0; ry < n; ry++) {
            String row = rows[ry];
            for (int i = 0; i < row.length(); i++) {
                char ch = row.charAt(i);
                if (ch == ' ') continue;
                int c = colour(ch, pal);
                if (c == 0) continue;
                int x = dir >= 0 ? x0 + i : x0 + w - 1 - i;
                f.set(x, feet - n + 1 + ry, c);
            }
        }
    }

    /** pal: by letter, see the art keys above. */
    private static int colour(char ch, int[] pal) {
        int i = "wbhsercmdlakx".indexOf(ch);
        return i >= 0 && i < pal.length ? pal[i] : 0;
    }

    static int[] pal(int w, int b, int h, int s, int e, int r, int c, int m, int d, int l, int a, int k) {
        return new int[] {w, b, h, s, e, r, c, m, d, l, a, k};
    }
}
