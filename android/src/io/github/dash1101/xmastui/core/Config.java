package io.github.dash1101.xmastui.core;

/** The wallpaper's settings (the Android side fills it from its preferences). */
public final class Config {
    /** Frames per second (24 by default). */
    public int fps = 24;
    /** Snowfall, percent (100 = normal). */
    public int snow = 100;
    /** The snowy shelf under the app dock. */
    public boolean dock = true;
    /** Its top edge, percent of the screen height from the bottom. */
    public int dockLift = 15;
    /** Its height, percent of the screen height. */
    public int dockHeight = 9;
    /** Its width, percent of the screen width (centred). */
    public int dockWidth = 92;
    /** "frost", "wood", "candy" or "stone". */
    public String dockStyle = "frost";
    /** How high the snow may pile on it (pixels at wallpaper resolution). */
    public int dockPile = 6;
    /** The villager's name ("" = people say "neighbour"). */
    public String name = "";
    /** "cat", "dog", "bunny" or "none"; and its name. */
    public String pet = "cat";
    public String petName = "";
    /** Speech bubbles over people's heads. */
    public boolean bubbles = true;
    /** "auto" (the phone's clock), "night", "dusk", "day" or "dawn". */
    public String time = "auto";
    /** Wallpaper pixels across the screen's short side (the size setting: 155 at 100%). */
    public int detail = 155;
    /** The most folk about at once (0: as many as fit the village). */
    public int mostPeople = 0;
    /** Friends & family: folk who come by under these names. */
    public java.util.List<Friend> friends = new java.util.ArrayList<>();
    /** Only friends & family come by. */
    public boolean onlyFriends = false;

    /** Someone in the friends & family list: a name, a kid or not, a coat and hat colour name. */
    public static final class Friend {
        public final String name, coat, hat;
        public final boolean kid;

        public Friend(String name, boolean kid, String coat, String hat) {
            this.name = name;
            this.kid = kid;
            this.coat = coat == null ? "" : coat;
            this.hat = hat == null ? "" : hat;
        }

        @Override
        public boolean equals(Object o) {
            if (!(o instanceof Friend)) return false;
            Friend f = (Friend) o;
            return f.name.equals(name) && f.kid == kid && f.coat.equals(coat) && f.hat.equals(hat);
        }

        @Override
        public int hashCode() {
            return name.hashCode() * 31 + (kid ? 1 : 0);
        }
    }

    /** Who's about changed (a new cast of folk). */
    boolean folkDiffer(Config o) {
        return o.mostPeople != mostPeople || o.onlyFriends != onlyFriends || !o.friends.equals(friends);
    }

    public Config copy() {
        Config c = new Config();
        c.fps = fps;
        c.snow = snow;
        c.dock = dock;
        c.dockLift = dockLift;
        c.dockHeight = dockHeight;
        c.dockWidth = dockWidth;
        c.dockStyle = dockStyle;
        c.dockPile = dockPile;
        c.name = name;
        c.pet = pet;
        c.petName = petName;
        c.bubbles = bubbles;
        c.time = time;
        c.detail = detail;
        c.mostPeople = mostPeople;
        c.friends = new java.util.ArrayList<>(friends);
        c.onlyFriends = onlyFriends;
        return c;
    }

    /** Who the villager is, to the others. */
    String you() {
        String n = name == null ? "" : name.trim();
        return n.isEmpty() ? "neighbour" : n;
    }

    String petCalled() {
        String n = petName == null ? "" : petName.trim();
        if (!n.isEmpty()) return n;
        if ("dog".equals(pet)) return "Biscuit";
        if ("bunny".equals(pet)) return "Clover";
        return "Mittens";
    }
}
