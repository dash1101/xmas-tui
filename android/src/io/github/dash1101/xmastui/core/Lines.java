package io.github.dash1101.xmastui.core;

/** What the village says. {a}/{b}: the two talking, {you}: the villager, {pet}, {days}. */
final class Lines {
    private Lines() {}

    static final String[] HELLO = {"evening, {b}!", "{b}! hello!", "oh, hi {b}!", "merry Christmas, {b}!", "{b}! lovely night"};
    static final String[] HELLO_BACK = {"hello, {a}!", "hi! cold one, eh?", "{a}! good to see you", "merry Christmas to you!", "oh! hello, {a}"};

    /** Little exchanges: a line, then the reply. */
    static final String[][] TALK = {
        {"it's really coming down", "good. I want a snow day"},
        {"{days}", "and I've wrapped nothing"},
        {"seen the tree lights?", "they twinkle like they're listening"},
        {"I could murder a cocoa", "with marshmallows. obviously"},
        {"my nose is frozen solid", "it's very red. festive!"},
        {"did you hear the bells?", "every hour, like clockwork"},
        {"the stars are so clear", "make a wish, quick!"},
        {"baking anything?", "gingerbread. come by later!"},
        {"have you met {you}?", "everyone's met {you}"},
        {"it smells like snow", "it smells like cinnamon"},
    };
    static final String[][] KID_TALK = {
        {"snowball fight?", "you're ON"},
        {"is Santa real?", "he ate my cookies, so yes"},
        {"I want a sled", "I want a puppy. AND a sled"},
        {"race you to the tree!", "no fair, you started!"},
        {"I can catch snowflakes", "on your TONGUE?"},
    };
    static final String[] BYE = {"see you!", "stay warm!", "night night!", "bye!", "merry Christmas!"};

    // the villager
    static final String[] YOU_HELLO = {"hi {b}!", "hello, {b}!", "evening, {b}!", "{b}! merry Christmas!"};
    static final String[] TO_YOU = {"oh, it's {you}! hello!", "{you}! how are you?", "there you are, {you}!", "hello hello, {you}!", "{you}! come here, you're freezing"};
    static final String[] YOU_REPLY = {"all cozy, thanks!", "never better!", "so much snow!", "I love this time of year"};
    static final String[] PET_HELLO = {"and hello, {pet}!", "who's this? {pet}!", "{pet}! so fluffy"};
    static final String[] FETCH = {"fetch, {pet}!", "go on, {pet}!", "catch!", "ready, {pet}?"};
    static final String[] FEED = {"here you go, little one", "a carrot for you", "easy now… there"};
    static final String[] WISH = {"a shooting star!", "make a wish!", "did you see that?!", "ooh, a wish!"};
    static final String[] TREE = {"the lights are so pretty", "best tree yet", "I could look at it all night", "it glows all the way up"};
    static final String[] TAPPED = {"oh! hello there!", "*waves*", "who's that?", "merry Christmas!", "hee hee, that tickles"};
    static final String[] SNOWBALL = {"gotcha!", "hey!!", "ha! missed me!", "right in the hat!"};

    static String fill(String s, String a, String b, String you, String pet, int days) {
        if ("{days}".equals(s)) {
            s = days <= 0 ? "it's Christmas!!" : (days == 1 ? "Christmas tomorrow!" : days + " days to Christmas!");
        }
        return s.replace("{a}", a).replace("{b}", b).replace("{you}", you).replace("{pet}", pet);
    }
}
