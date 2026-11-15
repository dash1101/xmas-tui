package io.github.dash1101.xmastui.core;

/** The villager's pet: at their heels, off after a thrown toy, napping when they stand about. */
final class PetSim {
    final String kind;
    float x, y, dir = 1f, age;
    boolean walking, napping, carrying;
    float stillFor, happy;
    // a toy in flight or on the ground
    boolean toy;
    float toyX, toyY, toyH, toyVx, toyVh;
    int fetchStep;

    PetSim(String kind, float x, float y) {
        this.kind = kind;
        this.x = x;
        this.y = y;
    }

    /** The villager throws its toy (a ball, yarn, or a carrot for a bunny). */
    void fetch(Village v, float fromX, float fromY, float dir) {
        toy = true;
        toyX = fromX;
        toyY = fromY;
        toyH = 6f;
        toyVx = dir * v.rnd.range(14f, 24f);
        toyVh = 14f;
        fetchStep = 0;
        napping = false;
        carrying = false;
    }

    void update(Village v, float dt) {
        age += dt;
        Village.Person p = v.you;
        boolean moving = p.st == Village.Person.WALK;
        stillFor = moving ? 0f : stillFor + dt;
        if (moving) napping = false;
        float tx = p.x - p.dir * 6f, ty = p.y;
        float speed = 1f;
        if (toy) {
            if (!carrying) {
                // the toy's flight: an arc, then a roll to a stop
                if (toyH > 0f || toyVh > 0f) {
                    toyVh -= 36f * dt;
                    toyH += toyVh * dt;
                    toyX += toyVx * dt;
                    if (toyH <= 0f) {
                        toyH = 0f;
                        toyVh = 0f;
                        toyVx *= 0.4f;
                    }
                } else {
                    toyVx *= Math.max(0f, 1f - 3f * dt);
                    toyX += toyVx * dt;
                }
                tx = toyX;
                ty = toyY;
                speed = 1.8f;
                if (Math.abs(toyX - x) < 1.5f && toyH < 2f) {
                    if ("cat".equals(kind) && fetchStep < 2) {
                        // a cat bats it about first
                        toyVx = (fetchStep % 2 == 0 ? 1f : -1f) * 10f;
                        toyVh = 6f;
                        toyH = 0.5f;
                        fetchStep++;
                        happy = v.t + 0.6f;
                    } else if ("bunny".equals(kind)) {
                        toy = false;
                        happy = v.t + 1.5f;
                        v.heart(x, y - 7);
                    } else {
                        carrying = true;
                    }
                }
            } else {
                toyX = x + dir * 3f;
                toyY = y;
                toyH = 3f;
                speed = 1.4f;
                if (Math.abs(x - tx) < 3f) {
                    toy = false;
                    carrying = false;
                    happy = v.t + 1.5f;
                    v.heart(x, y - 7);
                }
            }
        } else if (stillFor > 18f) {
            napping = true;
        }
        if (napping) {
            walking = false;
            return;
        }
        float dx = tx - x;
        walking = Math.abs(dx) > 0.8f;
        if (walking) {
            float vel = Math.min(Math.max(Math.abs(dx) * 3f, 4f), 40f) * speed;
            x += Math.signum(dx) * Math.min(Math.abs(dx), vel * dt);
            dir = Math.signum(dx);
        } else if (!moving) {
            dir = Math.signum(p.x - x);
            if (dir == 0f) dir = 1f;
        }
        y += (ty - y) * (1f - (float) Math.exp(-dt * 4f));
    }

    void draw(Village v, Pix f, float sun) {
        String[][] art = "dog".equals(kind) ? Art.DOG : ("bunny".equals(kind) ? Art.BUNNY : Art.CAT);
        int frame = napping ? 3 : (v.t < happy ? 2 : (walking ? ((int) (age * 7)) % 2 : 2));
        float shade = 0.7f + 0.3f * sun;
        int body, marks;
        if ("dog".equals(kind)) {
            body = Pix.hex(0xd8a050);
            marks = Pix.hex(0x8a5a28);
        } else if ("bunny".equals(kind)) {
            body = Pix.hex(0xe8e0d4);
            marks = Pix.hex(0xc8b8a8);
        } else {
            body = Pix.hex(0xe08a3a);
            marks = Pix.hex(0xa85a20);
        }
        int[] pal = Art.pal(Pix.hex(0xf8f6f0), Pix.mul(body, shade), 0, 0, 0, 0, 0, Pix.mul(marks, shade), Pix.hex(0x141014), 0, 0, Pix.hex(0x2fae5c));
        int hop = v.t < happy && ((int) (v.t * 6)) % 2 == 0 ? 1 : 0;
        Art.draw(f, art[frame], x, Math.round(y) - hop, dir, pal);
        if (napping && ((int) (v.t * 0.8f)) % 2 == 0) {
            f.set(Math.round(x) + 3, Math.round(y) - 7, Pix.hex(0xc8d0e0));
            f.set(Math.round(x) + 4, Math.round(y) - 8, Pix.hex(0xc8d0e0));
        }
        if (toy) {
            int c = "cat".equals(kind) ? Pix.hex(0xd84a6a) : ("bunny".equals(kind) ? Pix.hex(0xf07a28) : Pix.hex(0xc8e040));
            int tx = Math.round(toyX), ty = Math.round(toyY - toyH) - 1;
            f.set(tx, ty, c);
            f.set(tx + 1, ty, c);
            f.set(tx, ty - 1, c);
            f.set(tx + 1, ty - 1, Pix.mul(c, 1.2f));
        }
    }
}
