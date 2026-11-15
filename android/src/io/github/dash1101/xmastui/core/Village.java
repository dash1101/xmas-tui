package io.github.dash1101.xmastui.core;

import java.util.ArrayList;
import java.util.Calendar;
import java.util.List;

/**
 * The village on a phone: a night (or day) sky, mountains and pines, a row
 * of cottages, the big tree, and folk going about their evening on their own
 * — chatting, visiting, admiring the tree — with the villager among them,
 * greeting people by name and playing with their pet. Snow falls on
 * everything, and piles up on a shelf under the app dock.
 *
 * Pure Java (no Android): `update` steps it, `render` draws it into a
 * {@link Pix}, and `bubbles` lists the speech bubbles to draw on top as text.
 */
public final class Village {
    /** Wallpaper pixels on screen, and the world (a little wider: it pans with the home screens). */
    public final int w, h;
    final int ww;
    Config cfg;
    final Rnd rnd;
    float t;
    float cam, camTarget;

    // layout (pixel rows)
    final int horizon, forest, base, fieldBot;
    int dockTop, dockBot, dockX0, dockX1;

    // scenery
    final float[] farRidge, nearRidge;
    final List<House> houses = new ArrayList<>();
    final List<Lamp> lamps = new ArrayList<>();
    final int[] pines;
    int treeX, treeH;
    int snowmanX, snowmanY;
    final float[][] stars;

    // life
    final List<Person> people = new ArrayList<>();
    Person you;
    PetSim pet;
    final List<Animal> animals = new ArrayList<>();
    final List<Flake> flakes = new ArrayList<>();
    final List<Bubble> bubbles = new ArrayList<>();
    final List<Spark> sparks = new ArrayList<>();
    final List<Snowball> balls = new ArrayList<>();
    final List<Smoke> smoke = new ArrayList<>();
    float[] pile;
    float[] icicle;
    float starAt = 20f, starX, starY, starVx, starLife;
    float treeBlaze;
    float nextAnimal = 8f;
    float wind;

    public Village(int w, int h, Config cfg, long seed) {
        this.w = Math.max(40, w);
        this.h = Math.max(80, h);
        this.ww = Math.round(this.w * 1.7f);
        this.cfg = cfg.copy();
        this.rnd = new Rnd(seed);
        horizon = Math.round(this.h * 0.50f);
        forest = Math.round(this.h * 0.555f);
        base = Math.round(this.h * 0.655f);
        layoutDock();
        fieldBot = cfg.dock ? dockTop - 3 : Math.round(this.h * 0.93f);
        farRidge = new float[ww];
        nearRidge = new float[ww];
        int s = (int) (seed & 0xffff);
        for (int x = 0; x < ww; x++) {
            // peaks: ridged noise, sharp at the top
            float pk = 1f - Math.abs(2f * fbm(x * 0.018f, s) - 1f);
            farRidge[x] = horizon - 4 - this.h * 0.16f * pk * pk - 6f * Rnd.noise(x * 0.09f, s + 3);
            float pn = 1f - Math.abs(2f * fbm(x * 0.03f + 40f, s + 9) - 1f);
            nearRidge[x] = horizon + 6 - this.h * 0.07f * pn * pn - 3f * Rnd.noise(x * 0.13f, s + 5);
        }
        pines = new int[ww / 4];
        for (int i = 0; i < pines.length; i++) pines[i] = 6 + (int) (Rnd.h01(i * 31 + s) * 10);
        stars = new float[60][];
        for (int i = 0; i < stars.length; i++) stars[i] = new float[] {rnd.range(0, this.w), rnd.range(0, horizon - 10), rnd.range(0, 6.28f), rnd.range(0.4f, 1f)};
        buildVillage();
        cast();
        cam = camTarget = (ww - this.w) / 2f;
        for (int i = 0; i < 6; i++) step(0.5f);
    }

    private static float fbm(float x, int s) {
        return 0.6f * Rnd.noise(x, s) + 0.3f * Rnd.noise(x * 2.1f, s + 1) + 0.1f * Rnd.noise(x * 4.3f, s + 2);
    }

    private void layoutDock() {
        dockBot = Math.round(h * (1f - cfg.dockLift / 100f + cfg.dockHeight / 100f));
        dockBot = Math.min(h - 1, dockBot);
        dockTop = Math.round(h * (1f - cfg.dockLift / 100f));
        dockX0 = Math.round(w * (100 - Math.max(30, Math.min(100, cfg.dockWidth))) / 200f);
        dockX1 = w - dockX0;
        pile = new float[w];
        icicle = new float[w];
        Rnd r = new Rnd(97);
        for (int x = dockX0 + 2; x < dockX1 - 2; x++) icicle[x] = r.chance(0.18f) ? r.range(1f, 5f) : 0f;
    }

    /** New settings: what can change live does; the rest takes a fresh village. */
    public void setConfig(Config c) {
        boolean dockMoved = c.dock != cfg.dock || c.dockLift != cfg.dockLift || c.dockHeight != cfg.dockHeight || c.dockWidth != cfg.dockWidth;
        String oldPet = cfg.pet;
        cfg = c.copy();
        if (dockMoved) layoutDock();
        if (!oldPet.equals(cfg.pet)) pet = "none".equals(cfg.pet) ? null : new PetSim(cfg.pet, you.x - 6, you.y);
        if (you != null) you.name = cfg.you();
    }

    /** The home screen's page position, 0..1 (the village pans with it). */
    public void setPan(float x) {
        camTarget = Math.max(0f, Math.min(1f, x)) * (ww - w);
    }

    // ------------------------------------------------------------------ the village

    static final int[] WALLS = {0x8a4a3a, 0x3a5a7a, 0x6a7a4a, 0x9a7a5a, 0x5a4a6a, 0xb8a080, 0x7a3a3a};
    static final int[] ROOFS = {0x4a2a2a, 0x2a3a4a, 0x3a4a2a, 0x5a3a2a, 0x3a2a3a};

    private void buildVillage() {
        int x = 6;
        int i = 0;
        // the big tree goes in the middle of the world, houses either side
        treeX = ww / 2;
        treeH = Math.round(h * 0.25f);
        while (x < ww - 30) {
            int hw = rnd.i(30, 42);
            if (Math.abs((x + hw / 2) - treeX) < hw / 2 + 22) {
                x = treeX + 24;
                continue;
            }
            House hs = new House();
            hs.x = x;
            hs.w = hw;
            hs.wall = Pix.hex(WALLS[(i + rnd.i(0, 3)) % WALLS.length]);
            hs.roof = Pix.hex(ROOFS[rnd.i(0, ROOFS.length)]);
            hs.wallH = rnd.i(Math.round(h * 0.065f), Math.round(h * 0.09f));
            hs.roofH = rnd.i(Math.round(h * 0.045f), Math.round(h * 0.065f));
            hs.door = x + hw / 2 + rnd.i(-6, 7);
            hs.chimney = x + rnd.i(4, hw - 6);
            hs.lit = new boolean[6];
            for (int k = 0; k < 6; k++) hs.lit[k] = rnd.chance(0.75f);
            houses.add(hs);
            if (rnd.chance(0.7f)) {
                Lamp l = new Lamp();
                l.x = x + hw + rnd.i(3, 8);
                lamps.add(l);
            }
            x += hw + rnd.i(10, 22);
            i++;
        }
        snowmanX = treeX + 26;
        snowmanY = base + 10;
    }

    static final String[] NAMES = {"Rosa", "Ned", "Mabel", "Otto", "June", "Felix", "Ivy", "Hugo", "Clara", "Pip", "Wren", "Arlo"};
    static final int[] COATS = {0xb3202a, 0x1f6e3a, 0x2a4c8a, 0x6a3a8a, 0xc8862a, 0x3a6a6a, 0x8a5a2a};
    static final int[] HATS = {0xd8283a, 0x2fae5c, 0x3a7ad8, 0xe0a030, 0xd860a0, 0xf0f0f0};
    static final int[] HAIR = {0x3a2a1a, 0x8a5a2a, 0xd8d0c0, 0x1a1a1a, 0xa8462a, 0xe0c080};

    private void cast() {
        you = new Person(cfg.you(), true, false);
        you.coat = Pix.hex(0xe0a030);
        you.hat = Pix.hex(0x2fae5c);
        you.scarf = Pix.hex(0xd8283a);
        you.x = (ww - this.w) / 2f + this.w * 0.35f;
        you.y = base + 12;
        people.add(you);
        int n = Math.max(5, Math.min(8, ww / 36));
        List<String> names = new ArrayList<>();
        for (String s : NAMES) names.add(s);
        for (int i = 0; i < n; i++) {
            String name = names.remove(rnd.i(0, names.size()));
            boolean kid = i % 3 == 2;
            Person p = new Person(name, false, kid);
            p.coat = Pix.hex(COATS[rnd.i(0, COATS.length)]);
            p.hat = rnd.chance(0.6f) || kid ? Pix.hex(HATS[rnd.i(0, HATS.length)]) : 0;
            p.hair = Pix.hex(HAIR[rnd.i(0, HAIR.length)]);
            p.x = rnd.range(10, ww - 10);
            p.y = rnd.range(base + 6, fieldBot);
            p.lantern = !kid && rnd.chance(0.5f);
            people.add(p);
        }
        if (!"none".equals(cfg.pet)) pet = new PetSim(cfg.pet, you.x - 6, you.y);
    }

    // ------------------------------------------------------------------ simulation

    public void update(float dt) {
        dt = Math.min(dt, 0.25f);
        // larger steps in pieces so nothing tunnels
        while (dt > 0.05f) {
            step(0.05f);
            dt -= 0.05f;
        }
        step(dt);
    }

    private void step(float dt) {
        t += dt;
        cam += (camTarget - cam) * (1f - (float) Math.exp(-dt * 6f));
        wind = 2.2f * (Rnd.noise(t * 0.08f, 11) - 0.5f) + 0.6f;
        for (Person p : people) think(p, dt);
        meetings();
        if (pet != null) pet.update(this, dt);
        updateAnimals(dt);
        updateSnow(dt);
        updateBits(dt);
        // a shooting star now and then: anyone about looks up
        starAt -= dt;
        if (starAt <= 0f && starLife <= 0f) {
            shootingStar(rnd.range(10, w - 10), rnd.range(4, horizon * 0.6f));
        }
        if (starLife > 0f) {
            starLife -= dt;
            starX += starVx * dt;
            starY += 14f * dt;
        }
        treeBlaze = Math.max(0f, treeBlaze - dt);
    }

    void shootingStar(float x, float y) {
        starX = x;
        starY = y;
        starVx = rnd.chance(0.5f) ? 60f : -60f;
        starLife = 1.1f;
        starAt = rnd.range(25f, 60f);
        // someone remarks on it
        Person best = null;
        for (Person p : people) if (p.free() && !p.inside() && (best == null || rnd.chance(0.4f))) best = p;
        if (best != null) {
            best.look(t + 2.5f);
            say(best, rnd.pick(Lines.WISH), t + 0.3f);
        }
    }

    private void think(Person p, float dt) {
        p.age += dt;
        p.cool -= dt;
        switch (p.st) {
            case Person.WALK: {
                float dx = p.tx - p.x, dy = p.ty - p.y;
                float d = (float) Math.sqrt(dx * dx + dy * dy);
                float v = (p.kid ? 11f : 8f) * dt;
                if (d <= v) {
                    p.x = p.tx;
                    p.y = p.ty;
                    arrive(p);
                } else {
                    p.x += dx / d * v;
                    p.y += dy / d * v * 0.5f;
                    if (Math.abs(dx) > 0.3f) p.dir = Math.signum(dx);
                }
                break;
            }
            case Person.INSIDE:
                if (t > p.until) {
                    p.st = Person.IDLE;
                    p.until = t + 1.5f;
                    p.y = base + 5;
                    if (rnd.chance(0.5f)) say(p, p.kid ? "back outside!" : "brr! chilly", t + 0.2f);
                }
                break;
            case Person.CHAT:
                if (p.chat == null || p.chat.done) {
                    p.chat = null;
                    p.st = Person.IDLE;
                    p.until = t + rnd.range(1f, 2.5f);
                }
                break;
            default:
                if (t > p.until) decide(p);
        }
        if (p.chat != null) p.chat.update(this);
    }

    /** What next, for someone standing about. */
    private void decide(Person p) {
        float r = rnd.f();
        if (p.you) {
            // the villager: greet someone, play with the pet, feed a deer, admire the tree
            Person other = nearestFree(p, 200f);
            if (other != null && !onScreen(other.x)) other = null;
            Animal deer = animals.isEmpty() ? null : animals.get(0);
            if (deer != null && (deer.kind != Animal.DEER || !onScreen(deer.x))) deer = null;
            if (pet != null && r < 0.3f) {
                walkTo(p, Math.max(cam + 20, Math.min(cam + w - 20, p.x + rnd.range(-20, 20))), rnd.range(base + 8, fieldBot), Person.PLAY);
            } else if (deer != null && r < 0.5f) {
                walkTo(p, deer.x - 8 * deer.dir, deer.y, Person.FEED);
                p.target = deer;
            } else if (other != null && r < 0.8f) {
                walkTo(p, other.x - 7, other.y, Person.GREET);
                p.friend = other;
            } else if (onScreen(treeX)) {
                walkTo(p, treeX + rnd.range(-14, 14), base + rnd.range(6, 12), Person.WARM);
            } else {
                walkTo(p, inView(14), rnd.range(base + 6, fieldBot), Person.IDLE);
            }
            return;
        }
        if (r < 0.12f && !houses.isEmpty()) {
            House hs = houses.get(rnd.i(0, houses.size()));
            walkTo(p, hs.door, base + 3, Person.INSIDE);
            p.house = hs;
        } else if (r < 0.26f) {
            walkTo(p, treeX + rnd.range(-16, 16), base + rnd.range(5, 12), Person.WARM);
        } else if (r < 0.36f && p.kid) {
            walkTo(p, snowmanX + rnd.range(-8, 8), snowmanY + rnd.range(-2, 3), Person.WARM);
        } else if (r < 0.48f) {
            p.st = Person.IDLE;
            p.until = t + rnd.range(2f, 6f);
        } else if (rnd.chance(0.75f)) {
            // mostly about the part of the village on screen
            walkTo(p, inView(8), rnd.range(base + 6, fieldBot), Person.IDLE);
        } else {
            walkTo(p, clampX(p.x + rnd.range(-60, 60)), rnd.range(base + 6, fieldBot), Person.IDLE);
        }
    }

    private float clampX(float x) {
        return Math.max(6f, Math.min(ww - 6f, x));
    }

    /** Somewhere along the part of the village on screen (with a margin in). */
    private float inView(float margin) {
        return clampX(rnd.range(cam + margin, cam + w - margin));
    }

    private boolean onScreen(float x) {
        return x > cam + 4 && x < cam + w - 4;
    }

    private void walkTo(Person p, float x, float y, int then) {
        p.tx = clampX(x);
        p.ty = Math.max(base + 3, Math.min(fieldBot, y));
        p.then = then;
        p.st = Person.WALK;
    }

    /** Got where they were going. */
    private void arrive(Person p) {
        switch (p.then) {
            case Person.INSIDE:
                p.st = Person.INSIDE;
                p.until = t + rnd.range(6f, 14f);
                if (p.house != null) p.house.visits++;
                break;
            case Person.WARM:
                p.st = Person.WARM;
                p.until = t + rnd.range(4f, 9f);
                p.dir = Math.signum(treeX - p.x);
                if (rnd.chance(0.35f)) say(p, rnd.pick(Lines.TREE), t + 0.5f);
                break;
            case Person.PLAY:
                p.st = Person.IDLE;
                p.until = t + 5f;
                if (pet != null) {
                    say(p, Lines.fill(rnd.pick(Lines.FETCH), "", "", "", cfg.petCalled(), 0), t + 0.2f);
                    pet.fetch(this, p.x, p.y, p.dir);
                    p.wave = t + 0.6f;
                }
                break;
            case Person.FEED: {
                p.st = Person.IDLE;
                p.until = t + 3f;
                Animal a = p.target;
                if (a != null && animals.contains(a)) {
                    p.dir = Math.signum(a.x - p.x);
                    say(p, rnd.pick(Lines.FEED), t + 0.2f);
                    a.fed = t + 4f;
                    a.st = Animal.GRAZE;
                    heart(a.x, a.y - 12);
                }
                break;
            }
            case Person.GREET: {
                Person o = p.friend;
                p.st = Person.IDLE;
                p.until = t + 1f;
                if (o != null && o.free() && !o.inside()) startChat(p, o, true);
                break;
            }
            default:
                p.st = Person.IDLE;
                p.until = t + rnd.range(1.5f, 4f);
        }
    }

    private Person nearestFree(Person p, float r) {
        Person best = null;
        float bd = r;
        for (Person o : people) {
            if (o == p || !o.free() || o.inside() || o.cool > 0f) continue;
            float d = Math.abs(o.x - p.x);
            if (d < bd) {
                bd = d;
                best = o;
            }
        }
        return best;
    }

    /** Two people passing close by stop for a word (now and then). */
    private void meetings() {
        for (int i = 0; i < people.size(); i++) {
            Person a = people.get(i);
            if (!a.free() || a.inside() || a.cool > 0f) continue;
            for (int j = i + 1; j < people.size(); j++) {
                Person b = people.get(j);
                if (!b.free() || b.inside() || b.cool > 0f) continue;
                if (Math.abs(a.x - b.x) < 9f && Math.abs(a.y - b.y) < 4f) {
                    if (rnd.chance(0.5f)) startChat(a, b, a.you || b.you);
                    else {
                        a.cool = 6f;
                        b.cool = 6f;
                    }
                    return;
                }
            }
        }
    }

    private void startChat(Person a, Person b, boolean names) {
        a.st = Person.CHAT;
        b.st = Person.CHAT;
        a.dir = Math.signum(b.x - a.x);
        if (a.dir == 0f) a.dir = 1f;
        b.dir = -a.dir;
        // stand a step apart
        if (Math.abs(a.x - b.x) < 6f) b.x = a.x + 7f * a.dir;
        b.y = a.y + (b.y - a.y) * 0.3f;
        Chat c = new Chat(a, b);
        String you = cfg.you(), petName = cfg.petCalled();
        int days = daysToChristmas();
        if (a.you || b.you) {
            Person me = a.you ? a : b, them = a.you ? b : a;
            c.add(me, Lines.fill(rnd.pick(Lines.YOU_HELLO), me.name, them.name, you, petName, days));
            c.add(them, Lines.fill(rnd.pick(Lines.TO_YOU), them.name, me.name, you, petName, days));
            if (pet != null && rnd.chance(0.5f)) c.add(them, Lines.fill(rnd.pick(Lines.PET_HELLO), them.name, me.name, you, petName, days));
            c.add(me, Lines.fill(rnd.pick(Lines.YOU_REPLY), me.name, them.name, you, petName, days));
        } else {
            c.add(a, Lines.fill(rnd.pick(Lines.HELLO), a.name, b.name, you, petName, days));
            c.add(b, Lines.fill(rnd.pick(Lines.HELLO_BACK), a.name, b.name, you, petName, days));
        }
        String[] talk = (a.kid && b.kid) ? rnd.pick(Lines.KID_TALK) : rnd.pick(Lines.TALK);
        if (rnd.chance(0.75f)) {
            c.add(a, Lines.fill(talk[0], a.name, b.name, you, petName, days));
            c.add(b, Lines.fill(talk[1], a.name, b.name, you, petName, days));
        }
        boolean fight = a.kid && b.kid && talk == Lines.KID_TALK[0];
        c.add(b, rnd.pick(Lines.BYE));
        c.fight = fight;
        a.chat = c;
        b.chat = null;
        c.next = t + 0.4f;
    }

    /** Days until the 25th of December. */
    static int daysToChristmas() {
        Calendar now = Calendar.getInstance();
        Calendar x = Calendar.getInstance();
        x.set(now.get(Calendar.YEAR), Calendar.DECEMBER, 25, 0, 0, 0);
        long ms = x.getTimeInMillis() - now.getTimeInMillis();
        if (ms < -86400000L) {
            x.set(now.get(Calendar.YEAR) + 1, Calendar.DECEMBER, 25, 0, 0, 0);
            ms = x.getTimeInMillis() - now.getTimeInMillis();
        }
        return (int) Math.max(0, Math.ceil(ms / 86400000.0));
    }

    void say(Person p, String text, float at) {
        if (!cfg.bubbles) return;
        // one bubble per person at a time
        for (Bubble b : bubbles) if (b.who == p && b.at <= at) b.life = Math.min(b.life, at - b.at);
        Bubble b = new Bubble();
        b.who = p;
        b.text = text;
        b.at = at;
        b.life = 1.8f + text.length() * 0.07f;
        bubbles.add(b);
    }

    void heart(float x, float y) {
        Spark s = new Spark();
        s.x = x;
        s.y = y;
        s.vx = rnd.range(-3f, 3f);
        s.vy = -6f;
        s.life = 1.6f;
        s.c = Pix.hex(0xff6b8a);
        s.heart = true;
        sparks.add(s);
    }

    void burst(float x, float y, int n, int c) {
        for (int i = 0; i < n; i++) {
            Spark s = new Spark();
            s.x = x;
            s.y = y;
            float a = rnd.range(0, 6.28f), v = rnd.range(6f, 20f);
            s.vx = (float) Math.cos(a) * v;
            s.vy = (float) Math.sin(a) * v - 8f;
            s.life = rnd.range(0.5f, 1.1f);
            s.c = c;
            sparks.add(s);
        }
    }

    private void updateAnimals(float dt) {
        nextAnimal -= dt;
        if (nextAnimal <= 0f && animals.size() < 3) {
            nextAnimal = rnd.range(25f, 50f);
            Animal a = new Animal();
            a.kind = rnd.chance(0.5f) ? Animal.DEER : Animal.RABBIT;
            a.dir = rnd.chance(0.5f) ? 1f : -1f;
            a.x = a.dir > 0 ? -10 : ww + 10;
            a.y = rnd.range(base + 8, fieldBot - 2);
            a.until = t + rnd.range(6f, 12f);
            a.leave = t + rnd.range(30f, 60f);
            animals.add(a);
        }
        for (int i = animals.size() - 1; i >= 0; i--) {
            Animal a = animals.get(i);
            a.age += dt;
            if (a.st == Animal.WALK) {
                float v = (a.kind == Animal.DEER ? 6f : 9f) * dt;
                if (a.kind == Animal.RABBIT && (a.age * 3f) % 1f > 0.5f) v = 0f;
                a.x += a.dir * v;
                if (t > a.until && t < a.leave) {
                    a.st = Animal.GRAZE;
                    a.until = t + rnd.range(3f, 7f);
                }
                if (t < a.leave && (a.x < 8 || a.x > ww - 8)) a.dir = -a.dir;
            } else if (t > a.until && t > a.fed) {
                a.st = Animal.WALK;
                a.until = t + rnd.range(5f, 10f);
                if (rnd.chance(0.3f)) a.dir = -a.dir;
            }
            if (a.x < -20 || a.x > ww + 20) animals.remove(i);
        }
    }

    private void updateSnow(float dt) {
        int want = Math.round(w * h / 260f * cfg.snow / 100f);
        while (flakes.size() < want) {
            Flake f = new Flake();
            f.layer = rnd.i(0, 3);
            f.x = rnd.range(-10, w + 10);
            f.y = flakes.size() < want / 2 && t < 1f ? rnd.range(0, h) : rnd.range(-20, 0);
            f.v = (6f + 6f * f.layer) * rnd.range(0.8f, 1.2f);
            f.ph = rnd.range(0, 6.28f);
            flakes.add(f);
        }
        while (flakes.size() > want) flakes.remove(flakes.size() - 1);
        float melt = dt * 0.004f;
        for (int x = 0; x < w; x++) pile[x] = Math.max(0f, pile[x] - melt);
        for (Flake f : flakes) {
            f.y += f.v * dt;
            f.x += (wind * (0.5f + 0.5f * f.layer) + (float) Math.sin(t * 1.3f + f.ph) * 1.2f) * dt * 4f;
            // the near flakes land on the dock's shelf and pile up
            if (cfg.dock && f.layer >= 1) {
                int x = Math.round(f.x);
                if (x > dockX0 && x < dockX1 - 1 && f.y >= dockTop - pile[x] && f.y < dockTop + 2) {
                    if (pile[x] < cfg.dockPile) {
                        pile[x] += f.layer == 2 ? 0.9f : 0.5f;
                    }
                    respawn(f);
                    continue;
                }
            }
            if (f.y > h + 2 || f.x < -14 || f.x > w + 14) respawn(f);
        }
        // the pile settles: no sharp steps
        for (int x = dockX0 + 1; x < dockX1 - 1; x++) {
            float l = pile[x - 1], r = pile[x + 1];
            if (pile[x] - l > 1.2f) {
                pile[x] -= 0.3f;
                pile[x - 1] += 0.3f;
            }
            if (pile[x] - r > 1.2f) {
                pile[x] -= 0.3f;
                pile[x + 1] += 0.3f;
            }
        }
        // a lump slides off an end now and then
        if (rnd.chance(dt * 0.05f)) {
            int x = rnd.chance(0.5f) ? dockX0 + 1 : dockX1 - 2;
            if (pile[x] > 2f) {
                pile[x] *= 0.4f;
                burstS(x, dockTop, 5, Pix.hex(0xeef4ff));
            }
        }
    }

    private void burstS(float x, float y, int n, int c) {
        burst(x + cam, y, n, c);
    }

    private void respawn(Flake f) {
        f.x = rnd.range(-10, w + 10);
        f.y = rnd.range(-12, -1);
    }

    private void updateBits(float dt) {
        for (int i = sparks.size() - 1; i >= 0; i--) {
            Spark s = sparks.get(i);
            s.life -= dt;
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            if (!s.heart) s.vy += 30f * dt;
            if (s.life <= 0f) sparks.remove(i);
        }
        for (int i = bubbles.size() - 1; i >= 0; i--) {
            Bubble b = bubbles.get(i);
            if (t > b.at + b.life) bubbles.remove(i);
        }
        for (int i = balls.size() - 1; i >= 0; i--) {
            Snowball b = balls.get(i);
            b.age += dt;
            b.x += b.vx * dt;
            b.h += b.vh * dt;
            b.vh -= 60f * dt;
            if (b.h <= 0f) {
                burst(b.x, b.y - 2, 6, Pix.hex(0xf2f6ff));
                if (b.target != null && Math.abs(b.target.x - b.x) < 6f) say(b.target, rnd.pick(Lines.SNOWBALL), t);
                balls.remove(i);
            }
        }
        // chimney smoke
        if (rnd.chance(dt * 4f) && !houses.isEmpty()) {
            House hs = houses.get(rnd.i(0, houses.size()));
            Smoke s = new Smoke();
            s.x = hs.chimney + 1;
            s.y = base - hs.wallH - hs.roofH * 0.6f;
            s.life = rnd.range(3f, 5f);
            smoke.add(s);
        }
        for (int i = smoke.size() - 1; i >= 0; i--) {
            Smoke s = smoke.get(i);
            s.age += dt;
            s.y -= 4f * dt;
            s.x += wind * 2f * dt;
            if (s.age > s.life) smoke.remove(i);
        }
    }

    /** Kids having a snowball fight after their chat. */
    void throwBall(Person from, Person at) {
        Snowball b = new Snowball();
        b.x = from.x;
        b.y = from.y;
        b.h = 8f;
        float flight = 0.6f;
        b.vx = (at.x - from.x) / flight;
        b.vh = 30f * flight * 0.5f + 8f / flight;
        b.target = at;
        balls.add(b);
        from.wave = t + 0.5f;
    }

    // ------------------------------------------------------------------ taps

    /** A tap on the wallpaper (wallpaper pixels): people wave, the tree blazes, a house's lights go, the sky gets a shooting star, snow puffs. */
    public void tap(float sx, float sy) {
        float wx = sx + cam;
        for (Person p : people) {
            if (p.inside()) continue;
            if (Math.abs(p.x - wx) < 5f && sy <= p.y + 1 && sy >= p.y - (p.kid ? 13 : 17)) {
                p.wave = t + 1.2f;
                if (p.free()) {
                    p.st = Person.IDLE;
                    p.until = t + 2f;
                }
                say(p, rnd.pick(Lines.TAPPED), t);
                return;
            }
        }
        if (Math.abs(wx - treeX) < treeH * 0.35f && sy < base && sy > base - treeH) {
            treeBlaze = 2.5f;
            burst(wx, sy, 14, Pix.hex(0xffd36b));
            return;
        }
        for (House hs : houses) {
            if (wx >= hs.x && wx < hs.x + hs.w && sy < base && sy > base - hs.wallH - hs.roofH) {
                for (int k = 0; k < hs.lit.length; k++) hs.lit[k] = !hs.lit[k];
                return;
            }
        }
        if (cfg.dock && sy >= dockTop - cfg.dockPile - 2 && sy <= dockBot && sx > dockX0 && sx < dockX1) {
            // knock the snow off the dock
            for (int x = Math.max(dockX0, (int) sx - 10); x < Math.min(dockX1, (int) sx + 10); x++) {
                if (pile[x] > 0.5f) burstS(x, dockTop - pile[x], 1, Pix.hex(0xeef4ff));
                pile[x] *= 0.2f;
            }
            return;
        }
        if (sy < horizon - 8) {
            shootingStar(sx, sy);
            return;
        }
        burst(wx, sy, 10, Pix.hex(0xeef4ff));
    }

    // ------------------------------------------------------------------ drawing

    /** Sky palette for the hour (or the chosen time of day): {top, horizon, snow, light}. */
    private float[] light;
    private float lightAt = -1f;
    private String lightMode = "";

    private float[] daylight() {
        String mode = cfg.time == null ? "auto" : cfg.time;
        if (light != null && mode.equals(lightMode) && t - lightAt < 5f) return light;
        lightMode = mode;
        lightAt = t;
        light = computeDaylight(mode);
        return light;
    }

    private static float[] computeDaylight(String mode) {
        float hr;
        if ("night".equals(mode)) hr = 22f;
        else if ("dusk".equals(mode)) hr = 17.2f;
        else if ("day".equals(mode)) hr = 13f;
        else if ("dawn".equals(mode)) hr = 7.2f;
        else {
            Calendar c = Calendar.getInstance();
            hr = c.get(Calendar.HOUR_OF_DAY) + c.get(Calendar.MINUTE) / 60f;
        }
        // sun: 0 night .. 1 noon; warm: dawn/dusk glow
        float sun = (float) Math.max(0, Math.sin((hr - 6.5f) / 11f * Math.PI));
        float warm = (float) Math.max(0, 1 - Math.abs(hr - 17f) / 1.4f) + (float) Math.max(0, 1 - Math.abs(hr - 7.5f) / 1.2f);
        return new float[] {sun, Math.min(1f, warm)};
    }

    public void render(Pix f) {
        float[] dl = daylight();
        float sun = dl[0], warm = dl[1];
        int camI = Math.round(cam);
        // sky
        int top = Pix.mix(Pix.hex(0x0a0f26), Pix.hex(0x4a7ad0), sun);
        int hor = Pix.mix(Pix.mix(Pix.hex(0x3a2a5a), Pix.hex(0xb8d4f0), sun), Pix.hex(0xe8907a), warm * 0.8f);
        for (int y = 0; y < h; y++) {
            int c = Pix.mix(top, hor, Math.min(1f, y / (float) horizon));
            int row = y * w;
            for (int x = 0; x < w; x++) f.p[row + x] = c;
        }
        // stars and the moon (fading in daylight)
        float night = 1f - Math.min(1f, sun * 2.5f);
        if (night > 0f) {
            for (float[] s : stars) {
                float tw = 0.55f + 0.45f * (float) Math.sin(t * 1.7f + s[2]);
                f.blendS((int) s[0], (int) s[1], Pix.hex(0xfff6dc), s[3] * tw * night);
            }
            // the aurora: soft curtains over the hills
            for (int x = 0; x < w; x++) {
                float wx = x + cam * 0.1f;
                float band = Rnd.noise(wx * 0.025f + t * 0.03f, 21);
                band = band * band * (0.6f + 0.4f * Rnd.noise(wx * 0.11f - t * 0.05f, 22));
                float base0 = horizon * (0.42f + 0.12f * Rnd.noise(wx * 0.01f + t * 0.01f, 23));
                float len = horizon * 0.22f;
                for (int y = Math.max(0, (int) (base0 - len)); y < base0 + 3 && y < h; y++) {
                    float u = (base0 - y) / len;
                    float a = u < 0 ? Math.max(0f, 1f + (base0 - y) / 3f) : (float) Math.pow(Math.max(0f, 1f - u), 1.4f);
                    int c = u < 0.45f ? Pix.mix(Pix.hex(0x3aff9a), Pix.hex(0x3ad8d0), Math.max(0f, u) / 0.45f) : Pix.mix(Pix.hex(0x3ad8d0), Pix.hex(0x9a6aff), (u - 0.45f) / 0.55f);
                    int i = y * w + x;
                    f.p[i] = Pix.add(f.p[i], c, a * band * 0.22f * night);
                }
            }
            float mx = w * 0.78f, my = horizon * 0.22f, mr = Math.max(5f, w * 0.045f);
            f.glowS(mx, my, mr * 5f, Pix.hex(0x2a3360), night);
            f.ox = 0;
            for (int y = (int) (my - mr - 1); y <= my + mr + 1; y++) {
                for (int x = (int) (mx - mr - 1); x <= mx + mr + 1; x++) {
                    float dx = x + 0.5f - mx, dy = y + 0.5f - my;
                    float d = (float) Math.sqrt(dx * dx + dy * dy);
                    if (d <= mr) {
                        float crater = 0.85f + 0.15f * Rnd.noise(x * 0.9f + y * 1.7f, 4);
                        f.blendS(x, y, Pix.mul(Pix.hex(0xf6efd6), crater), night * Math.min(1f, mr + 0.5f - d));
                    }
                }
            }
            if (starLife > 0f) {
                float k = Math.min(1f, starLife * 2f);
                for (int i = 0; i < 12; i++) {
                    float a = (1f - i / 12f) * k;
                    f.blendS(Math.round(starX - Math.signum(starVx) * i * 1.5f), Math.round(starY - i * 0.35f), Pix.hex(0xfff6dc), a);
                }
                f.glowS(starX, starY, 4f, Pix.hex(0xfff0c0), 0.6f * k);
            }
        }
        // mountains (slow parallax), then the near ridge
        int snowCap = Pix.mix(Pix.hex(0x9aa4c8), Pix.hex(0xf0f4ff), sun);
        int farRock = Pix.mix(Pix.hex(0x2a2e4a), Pix.hex(0x7a88a8), sun);
        int nearRock = Pix.mix(Pix.hex(0x1e2238), Pix.hex(0x5a6a88), sun);
        int fieldC = Pix.mix(Pix.hex(0x5a6488), Pix.hex(0xdfe8f6), sun);
        for (int x = 0; x < w; x++) {
            int fx = Math.min(ww - 1, Math.max(0, Math.round(x + cam * 0.25f)));
            int nx = Math.min(ww - 1, Math.max(0, Math.round(x + cam * 0.5f)));
            int y0 = Math.round(farRidge[fx]);
            for (int y = Math.max(0, y0); y < forest; y++) {
                boolean cap = y - y0 < 2 + (int) ((horizon - y0) * 0.35f) + (int) (Rnd.h01(fx * 13) * 3) && y0 < horizon - 10;
                f.setS(x, y, cap ? snowCap : farRock);
            }
            int y1 = Math.round(nearRidge[nx]);
            for (int y = Math.max(0, y1); y < base; y++) {
                float k = (y - y1) / (float) Math.max(1, base - y1);
                f.setS(x, y, Pix.mix(y - y1 < 2 ? snowCap : nearRock, fieldC, k * 0.6f));
            }
        }
        // the pines along the foot of the hills
        int pine = Pix.mix(Pix.hex(0x0f1c22), Pix.hex(0x2a4a3a), sun);
        for (int i = 0; i < pines.length; i++) {
            int px = Math.round(i * 4 - cam * 0.75f) + (int) (Rnd.h01(i * 7) * 3);
            if (px < -6 || px > w + 6) continue;
            int ph = pines[i];
            int py = forest + 6 + (int) (Rnd.h01(i * 17) * 4);
            for (int k = 0; k < ph; k++) {
                int hw = (k * 3) / ph + (k % 3 == 2 ? 1 : 0);
                for (int dx = -hw; dx <= hw; dx++) f.setS(px + dx, py - ph + k, k < 2 ? Pix.mix(pine, snowCap, 0.5f) : pine);
            }
        }
        // the snowfield
        for (int y = base - 2; y < h; y++) {
            float k = (y - base) / (float) (h - base);
            int c = Pix.mix(Pix.mul(fieldC, 0.92f), Pix.mul(fieldC, 1.06f), k);
            for (int x = 0; x < w; x++) {
                int wx = x + camI;
                float n = 0.97f + 0.06f * Rnd.h01(wx * 3 + y * 1777);
                f.setS(x, y, Pix.mul(c, n));
            }
        }
        f.ox = camI;
        float glow = 1f - Math.min(1f, sun * 1.6f);
        // houses
        for (House hs : houses) drawHouse(f, hs, sun, glow);
        // smoke
        for (Smoke s : smoke) {
            float u = s.age / s.life;
            f.disc(s.x, s.y, 0.8f + u * 2.5f, Pix.hex(0x9a9eb4), (1f - u) * 0.3f);
        }
        // lamps
        for (Lamp l : lamps) {
            int lx = l.x, ly = base + 2;
            for (int y = ly - 14; y < ly; y++) f.set(lx, y, Pix.hex(0x22242c));
            f.rect(lx - 1, ly - 16, 3, 2, Pix.hex(0xffd27a));
            if (glow > 0f) f.glow(lx + 0.5f, ly - 15, 8f, Pix.hex(0xffb050), 0.5f * glow);
        }
        drawTree(f, sun, glow);
        drawSnowman(f, sun);
        // people, animals and the pet, back to front
        List<Object> order = new ArrayList<>();
        for (Person p : people) if (!p.inside()) order.add(p);
        order.addAll(animals);
        if (pet != null) order.add(pet);
        order.sort((a, b) -> Float.compare(yOf(a), yOf(b)));
        for (Object o : order) {
            if (o instanceof Person) drawPerson(f, (Person) o, glow, sun);
            else if (o instanceof Animal) drawAnimal(f, (Animal) o, sun);
            else ((PetSim) o).draw(this, f, sun);
        }
        for (Snowball b : balls) {
            f.set(Math.round(b.x), Math.round(b.y - b.h), Pix.hex(0xf2f6ff));
            f.set(Math.round(b.x) + 1, Math.round(b.y - b.h), Pix.hex(0xdfe6f2));
        }
        for (Spark s : sparks) {
            float a = Math.min(1f, s.life * 2f);
            if (s.heart) {
                int hx = Math.round(s.x), hy = Math.round(s.y);
                f.blend(hx - 1, hy, s.c, a);
                f.blend(hx + 1, hy, s.c, a);
                f.blend(hx, hy + 1, s.c, a);
                f.blend(hx - 1, hy - 1, s.c, a * 0.8f);
                f.blend(hx + 1, hy - 1, s.c, a * 0.8f);
                f.blend(hx, hy, s.c, a);
            } else {
                f.blend(Math.round(s.x), Math.round(s.y), s.c, a);
            }
        }
        f.ox = 0;
        drawSnow(f, sun);
        if (cfg.dock) drawDock(f, sun);
    }

    private static float yOf(Object o) {
        if (o instanceof Person) return ((Person) o).y;
        if (o instanceof Animal) return ((Animal) o).y;
        return ((PetSim) o).y;
    }

    private void drawHouse(Pix f, House hs, float sun, float glow) {
        int wt = base - hs.wallH;
        int wall = Pix.mix(Pix.mul(hs.wall, 0.55f), hs.wall, 0.35f + 0.65f * sun);
        for (int y = wt; y < base; y++) {
            float course = (y - wt) % 3 == 0 ? 0.88f : 1f;
            for (int x = hs.x; x < hs.x + hs.w; x++) {
                float side = 0.86f + 0.14f * (x - hs.x) / hs.w;
                f.set(x, y, Pix.mul(wall, course * side));
            }
        }
        // the roof, snow along its top
        int roof = Pix.mix(Pix.mul(hs.roof, 0.6f), hs.roof, 0.4f + 0.6f * sun);
        int snow = Pix.mix(Pix.hex(0xa8b0d0), Pix.hex(0xf4f8ff), sun);
        float c = hs.x + hs.w / 2f;
        for (int k = 0; k < hs.roofH; k++) {
            int y = wt - hs.roofH + k;
            float hw = (hs.w / 2f + 2f) * (k + 1) / hs.roofH;
            for (int x = Math.round(c - hw); x <= Math.round(c + hw); x++) {
                float edge = Math.min(Math.abs(x - (c - hw)), Math.abs(x - (c + hw)));
                boolean snowy = k < 2 || edge < 1.5f || Rnd.h01(x * 31 + y * 7) > 0.94f;
                f.set(x, y, snowy ? snow : Pix.mul(roof, (y + x / 2) % 2 == 0 ? 1f : 0.88f));
            }
        }
        // chimney
        int chTop = wt - hs.roofH + Math.round(Math.abs(hs.chimney - c) / (hs.w / 2f + 2f) * hs.roofH) - 3;
        f.rect(hs.chimney, chTop, 2, 5, Pix.hex(0x5a2a22));
        f.set(hs.chimney, chTop, snow);
        f.set(hs.chimney + 1, chTop, snow);
        // windows: lit warm at night (dark in the day), crossed frames
        int nw = Math.max(2, hs.w / 13);
        int ww2 = 5, wh = 6;
        int wy = wt + Math.max(2, hs.wallH / 4);
        for (int k = 0; k < nw && k < hs.lit.length; k++) {
            int wx = hs.x + 3 + k * ((hs.w - 6 - ww2) / Math.max(1, nw - 1));
            if (Math.abs(wx + ww2 / 2 - hs.door) < 5) continue;
            boolean lit = hs.lit[k];
            float flick = 0.9f + 0.1f * (float) Math.sin(t * 3f + k + hs.x);
            int glass = lit ? Pix.mix(Pix.hex(0x5a4a3a), Pix.mul(Pix.hex(0xffc864), flick), Math.max(0.35f, glow)) : Pix.hex(0x1c2030);
            f.rect(wx, wy, ww2, wh, glass);
            int frame = Pix.hex(0x3a2214);
            for (int y = wy; y < wy + wh; y++) f.set(wx + ww2 / 2, y, frame);
            for (int x = wx; x < wx + ww2; x++) f.set(x, wy + wh / 2, frame);
            for (int x = wx - 1; x <= wx + ww2; x++) f.set(x, wy + wh, snow);
            if (lit && glow > 0f) f.glow(wx + ww2 / 2f, wy + wh / 2f, 9f, Pix.hex(0xffa040), 0.35f * glow);
        }
        // the door, a wreath, a warm step
        int dh = Math.min(9, hs.wallH - 3);
        f.rect(hs.door - 2, base - dh, 5, dh, Pix.hex(0x3b2316));
        f.set(hs.door + 1, base - dh / 2, Pix.hex(0xffc23a));
        f.set(hs.door, base - dh + 2, Pix.hex(0x2fae5c));
        f.set(hs.door - 1, base - dh + 2, Pix.hex(0x2fae5c));
        f.set(hs.door + 1, base - dh + 2, Pix.hex(0x2fae5c));
        f.set(hs.door, base - dh + 3, Pix.hex(0xd8283a));
        // string lights under the eaves
        for (int x = hs.x; x < hs.x + hs.w; x += 3) {
            int k = (x / 3 + (int) (t * 2f)) % 4;
            int lc = new int[] {0xff5a5a, 0x5ad07a, 0xffd36b, 0x5a9aff}[k];
            f.set(x, wt + 1, Pix.hex(lc));
            if (glow > 0.3f) f.glow(x + 0.5f, wt + 1.5f, 2.5f, Pix.hex(lc), 0.25f * glow);
        }
    }

    private void drawTree(Pix f, float sun, float glow) {
        int bx = treeX, by = base + 4;
        int green = Pix.mix(Pix.hex(0x0f3a22), Pix.hex(0x2a6a3a), sun);
        int dark = Pix.mul(green, 0.75f);
        int tiers = 4;
        for (int k = 0; k < tiers; k++) {
            float y0 = by - treeH + k * treeH * 0.22f;
            float y1 = y0 + treeH * 0.34f;
            float half = treeH * (0.14f + 0.09f * k);
            for (int y = Math.round(y0); y < Math.round(y1); y++) {
                float u = (y - y0) / (y1 - y0);
                float hw = half * u + 1;
                for (int x = Math.round(bx - hw); x <= Math.round(bx + hw); x++) {
                    boolean edge = Math.abs(x - bx) > hw - 1.2f;
                    f.set(x, y, edge || (x + y) % 5 == 0 ? dark : green);
                    if (u > 0.85f && Rnd.h01(x * 7 + y) > 0.6f) f.set(x, y, Pix.hex(0xe8f0ff));
                }
            }
        }
        f.rect(bx - 2, by - 3, 4, 4, Pix.hex(0x4a2a16));
        // lights: twinkling, and blazing when tapped
        int[] cols = {0xff5a5a, 0xffd36b, 0x5a9aff, 0x5ad07a, 0xff8ae0};
        float blaze = treeBlaze > 0f ? 1.6f : 1f;
        for (int i = 0; i < 46; i++) {
            float u = Rnd.h01(i * 13 + 5);
            float yy = by - treeH * 0.95f + u * treeH * 0.9f;
            float hw = treeH * 0.4f * (u * 0.95f + 0.05f);
            float xx = bx + (Rnd.h01(i * 29 + 1) - 0.5f) * 2f * hw;
            float tw = 0.5f + 0.5f * (float) Math.sin(t * 2.5f + i * 1.7f);
            int lc = Pix.hex(cols[i % cols.length]);
            f.set(Math.round(xx), Math.round(yy), Pix.mul(lc, 0.6f + 0.6f * tw * blaze));
            if (glow > 0f || treeBlaze > 0f) f.glow(xx + 0.5f, yy + 0.5f, 3f, lc, 0.18f * tw * Math.max(glow, treeBlaze > 0 ? 1f : 0f) * blaze);
        }
        // the star
        float sy = by - treeH - 1;
        int gold = Pix.hex(0xffd65a);
        f.set(bx, Math.round(sy) - 1, gold);
        f.set(bx - 1, Math.round(sy), gold);
        f.set(bx, Math.round(sy), gold);
        f.set(bx + 1, Math.round(sy), gold);
        f.set(bx, Math.round(sy) + 1, gold);
        f.glow(bx + 0.5f, sy + 0.5f, 7f, Pix.hex(0xffd36b), 0.45f + 0.15f * (float) Math.sin(t * 2f));
        // presents underneath
        int[] wraps = {0xb3202a, 0x2a5aa8, 0x2a8a4a, 0xc8a040};
        for (int k = 0; k < 4; k++) {
            int px = bx - 9 + k * 5, ph = 3 + k % 2;
            f.rect(px, by + 1 - ph, 4, ph, Pix.hex(wraps[k]));
            for (int y = by + 1 - ph; y < by + 1; y++) f.set(px + 2, y, Pix.hex(0xffe0a0));
        }
    }

    private void drawSnowman(Pix f, float sun) {
        int sx = snowmanX, sy = snowmanY;
        int snow = Pix.mix(Pix.hex(0xb8c0dc), Pix.hex(0xf8fbff), sun);
        f.disc(sx, sy - 4, 4.2f, snow, 1f);
        f.disc(sx, sy - 10, 3.2f, Pix.mul(snow, 1.02f), 1f);
        f.disc(sx, sy - 15, 2.4f, snow, 1f);
        f.set(sx - 1, sy - 16, Pix.hex(0x1a1a1a));
        f.set(sx + 1, sy - 16, Pix.hex(0x1a1a1a));
        f.set(sx + 2, sy - 15, Pix.hex(0xff8a2a));
        f.rect(sx - 2, sy - 18, 5, 1, Pix.hex(0x1a1a1a));
        f.rect(sx - 1, sy - 21, 3, 3, Pix.hex(0x1a1a1a));
        f.rect(sx - 3, sy - 13, 7, 1, Pix.hex(0xd8283a));
        f.set(sx, sy - 9, Pix.hex(0x1a1a1a));
        f.set(sx, sy - 7, Pix.hex(0x1a1a1a));
    }

    private void drawPerson(Pix f, Person p, float glow, float sun) {
        boolean step = p.st == Person.WALK && ((int) (p.age * (p.kid ? 9 : 6))) % 2 == 0;
        boolean up = t < p.wave;
        String[] rows = p.kid ? (up ? Art.KID_UP : Art.KID) : (up ? Art.ARMS_UP : (p.you ? Art.YOU : (p.hat != 0 ? Art.HATTED : Art.ADULT)));
        float shade = 0.75f + 0.25f * sun;
        int hat = p.hat != 0 ? p.hat : p.hair;
        int[] pal = Art.pal(Pix.hex(0xf4f0ea), Pix.mul(Pix.hex(0x2a2a34), shade), Pix.mul(hat, shade), Pix.mul(Pix.hex(0xe8c39e), shade), Pix.hex(0x2a1a14), Pix.mul(p.scarf != 0 ? p.scarf : p.coat, shade),
            Pix.mul(p.coat, shade), 0, 0, 0, 0, 0);
        int feet = Math.round(p.y);
        if (step) {
            // a bob while walking
            Art.draw(f, rows, p.x, feet - 1, p.dir, pal);
            f.set(Math.round(p.x) - 2 + (p.dir > 0 ? 1 : 0), feet, pal[1]);
        } else {
            Art.draw(f, rows, p.x, feet, p.dir, pal);
        }
        if (p.lantern) {
            int lx = Math.round(p.x) + (p.dir > 0 ? 3 : -4), ly = feet - 6;
            float fl = 0.85f + 0.15f * (float) Math.sin(t * 9f + p.x);
            f.rect(lx, ly, 2, 2, Pix.mul(Pix.hex(0xffd27a), fl));
            if (glow > 0f) f.glow(lx + 1, ly + 1, 9f, Pix.hex(0xffb050), 0.45f * glow * fl);
        }
        if (p.you && glow > 0f) f.glow(p.x, feet - 8, 10f, Pix.hex(0xffc878), 0.12f * glow);
    }

    private void drawAnimal(Pix f, Animal a, float sun) {
        float shade = 0.7f + 0.3f * sun;
        if (a.kind == Animal.DEER) {
            int fr = a.st == Animal.GRAZE ? 2 : (((int) (a.age * 6)) % 2);
            int[] pal = Art.pal(Pix.hex(0xf4f0ea), Pix.mul(Pix.hex(0x9a6a40), shade), 0, 0, 0, 0, 0, 0, Pix.hex(0x1a1010), Pix.mul(Pix.hex(0xe0c8a8), shade), Pix.mul(Pix.hex(0x5a3a22), shade), Pix.hex(0x2a1a10));
            Art.draw(f, Art.DEER[fr], a.x, Math.round(a.y), a.dir, pal);
        } else {
            int fr = a.st == Animal.GRAZE ? 2 : (((int) (a.age * 6)) % 2);
            int hop = a.st == Animal.WALK && (a.age * 3f) % 1f < 0.5f ? 1 : 0;
            int[] pal = Art.pal(Pix.hex(0xf4f0ea), Pix.mul(Pix.hex(0x9a8a7a), shade), 0, 0, 0, 0, 0, Pix.mul(Pix.hex(0x7a6a5a), shade), Pix.hex(0x1a1010), 0, 0, 0);
            Art.draw(f, Art.BUNNY[fr], a.x, Math.round(a.y) - hop, a.dir, pal);
        }
    }

    private void drawSnow(Pix f, float sun) {
        int c = Pix.mix(Pix.hex(0xdce4f8), Pix.hex(0xffffff), sun);
        for (Flake fl : flakes) {
            int x = Math.round(fl.x), y = Math.round(fl.y);
            float a = 0.45f + 0.25f * fl.layer;
            f.blendS(x, y, c, a);
            if (fl.layer == 2) f.blendS(x + 1, y, c, a * 0.6f);
        }
    }

    private void drawDock(Pix f, float sun) {
        String style = cfg.dockStyle == null ? "frost" : cfg.dockStyle;
        int x0 = dockX0, x1 = dockX1, y0 = dockTop, y1 = dockBot;
        for (int y = y0; y < y1; y++) {
            for (int x = x0; x < x1; x++) {
                // rounded ends
                int ex = Math.min(x - x0, x1 - 1 - x), ey = Math.min(y - y0, y1 - 1 - y);
                if (ex < 2 && ey < 2 && ex + ey < 2) continue;
                switch (style) {
                    case "wood": {
                        float grain = 0.86f + 0.14f * Rnd.noise(x * 0.35f + y * 3.1f, 6);
                        int plank = (y - y0) % 5 == 4 ? Pix.hex(0x3a2214) : Pix.mul(Pix.hex(0x8a5a34), grain);
                        f.setS(x, y, Pix.mul(plank, 0.7f + 0.3f * sun));
                        break;
                    }
                    case "candy": {
                        boolean red = ((x + y) / 4) % 2 == 0;
                        f.setS(x, y, red ? Pix.hex(0xd8283a) : Pix.hex(0xf4f0ea));
                        break;
                    }
                    case "stone": {
                        boolean mortar = (y - y0) % 4 == 3 || (x + ((y - y0) / 4) * 3) % 8 == 0;
                        float n = 0.85f + 0.15f * Rnd.h01(x * 5 + y * 11);
                        f.setS(x, y, mortar ? Pix.hex(0x4a4a58) : Pix.mul(Pix.hex(0x8a8a9a), n * (0.7f + 0.3f * sun)));
                        break;
                    }
                    default: {
                        // frosted glass: the scene shows through, paler
                        int under = f.getS(x, y);
                        int c = Pix.mix(under, Pix.hex(0xdfeaff), 0.38f);
                        if (y == y0 || y == y0 + 1) c = Pix.mix(c, Pix.hex(0xffffff), 0.35f);
                        f.setS(x, y, c);
                    }
                }
            }
        }
        // the edge highlight and a shadow underneath
        for (int x = x0 + 2; x < x1 - 2; x++) f.blendS(x, y1, Pix.hex(0x000000), 0.25f);
        // icicles along the underside
        int ice = Pix.hex(0xcfe6ff);
        for (int x = x0 + 2; x < x1 - 2; x++) {
            int n = Math.round(icicle[x]);
            for (int k = 0; k < n; k++) f.blendS(x, y1 + k, ice, 0.85f - k * 0.15f);
        }
        // the snow that's landed on top
        int snow = Pix.mix(Pix.hex(0xdfe6f8), Pix.hex(0xffffff), sun);
        for (int x = x0 + 1; x < x1 - 1; x++) {
            int n = Math.round(pile[x]);
            for (int k = 0; k < n; k++) {
                int y = y0 - 1 - k;
                f.setS(x, y, k == n - 1 ? snow : Pix.mul(snow, 0.92f - 0.02f * k));
            }
            if (n > 0 && Rnd.h01(x * 7 + (int) (t * 1.5f)) > 0.985f) f.setS(x, y0 - n, Pix.hex(0xffffff));
        }
    }

    // ------------------------------------------------------------------ speech bubbles

    /** A speech bubble to draw as text: what, and where (wallpaper pixels: the speaker's head). */
    public static final class Say {
        public final String text;
        public final float x, y, alpha;

        Say(String text, float x, float y, float alpha) {
            this.text = text;
            this.x = x;
            this.y = y;
            this.alpha = alpha;
        }
    }

    public List<Say> bubbles() {
        List<Say> out = new ArrayList<>();
        for (Bubble b : bubbles) {
            if (t < b.at || b.who == null || b.who.inside()) continue;
            float age = t - b.at;
            float a = Math.min(1f, Math.min(age * 4f, (b.life - age) * 3f));
            int shown = Math.min(b.text.length(), (int) (age * 30f) + 1);
            float head = b.who.y - (b.who.kid ? 13 : 17);
            out.add(new Say(b.text.substring(0, shown), b.who.x - cam, head - 2, Math.max(0f, a)));
        }
        return out;
    }

    // ------------------------------------------------------------------ the cast

    static final class Person {
        static final int WALK = 0, IDLE = 1, CHAT = 2, INSIDE = 3, WARM = 4, PLAY = 5, FEED = 6, GREET = 7, LOOK = 8;
        String name;
        final boolean you, kid;
        int coat, hat, hair, scarf;
        boolean lantern;
        float x, y, dir = 1f, age;
        int st = IDLE, then = IDLE;
        float until, tx, ty, cool, wave;
        House house;
        Person friend;
        Animal target;
        Chat chat;

        Person(String name, boolean you, boolean kid) {
            this.name = name;
            this.you = you;
            this.kid = kid;
        }

        boolean free() {
            return st == IDLE || st == WALK || st == WARM || st == LOOK;
        }

        boolean inside() {
            return st == INSIDE;
        }

        void look(float until) {
            if (free()) {
                st = LOOK;
                this.until = until;
            }
        }
    }

    /** A conversation: lines, said in turn. */
    static final class Chat {
        final Person a, b;
        final List<Person> who = new ArrayList<>();
        final List<String> lines = new ArrayList<>();
        int i;
        float next;
        boolean done, fight;

        Chat(Person a, Person b) {
            this.a = a;
            this.b = b;
        }

        void add(Person p, String line) {
            who.add(p);
            lines.add(line);
        }

        void update(Village v) {
            if (done || v.t < next) return;
            if (i >= lines.size()) {
                done = true;
                a.cool = v.rnd.range(25f, 45f);
                b.cool = v.rnd.range(25f, 45f);
                if (b.st == Person.CHAT) {
                    b.st = Person.IDLE;
                    b.until = v.t + 1.5f;
                }
                if (fight) {
                    v.throwBall(a, b);
                    v.throwBall(b, a);
                }
                if (a.you || b.you) a.wave = v.t + 0.8f;
                return;
            }
            String line = lines.get(i);
            Person p = who.get(i);
            v.say(p, line, v.t);
            next = v.t + 1.6f + line.length() * 0.065f;
            i++;
        }
    }

    static final class Animal {
        static final int DEER = 0, RABBIT = 1;
        static final int WALK = 0, GRAZE = 1;
        int kind, st = WALK;
        float x, y, dir = 1f, age, until, leave, fed;
    }

    static final class House {
        int x, w, wallH, roofH, door, chimney, wall, roof, visits;
        boolean[] lit;
    }

    static final class Lamp {
        int x;
    }

    static final class Flake {
        float x, y, v, ph;
        int layer;
    }

    static final class Bubble {
        Person who;
        String text;
        float at, life;
    }

    static final class Spark {
        float x, y, vx, vy, life;
        int c;
        boolean heart;
    }

    static final class Snowball {
        float x, y, h, vx, vh, age;
        Person target;
    }

    static final class Smoke {
        float x, y, age, life;
    }
}
