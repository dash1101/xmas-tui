//! Things the scene says: present contents, the snowman's chatter, Santa
//! over the moon, passers-by, wishes, houses. Hand-written pools plus
//! little phrase generators (thousands of combinations), with a short
//! memory so nothing repeats back-to-back.

use crate::rng::Rng;
use std::collections::VecDeque;

const GIFT_ADJ: &[&str] = &[
    "a tiny", "a slightly squished", "a suspiciously heavy", "a glittery", "a hand-knitted", "an enormous", "a mysterious",
    "a perfectly wrapped", "a lopsided", "a very shiny", "a slightly singed", "a hand-painted", "a wobbly", "a vintage",
    "a brand-new", "a glow-in-the-dark", "a pocket-sized", "a gently used", "a magical", "an extremely fluffy",
];
const GIFT_ITEM: &[&str] = &[
    "snow globe", "reindeer plushie", "box of cocoa", "pair of fuzzy socks", "candy cane", "jar of starlight", "sled",
    "gingerbread house", "string of lights", "mitten (just the left one)", "toy train", "book of terrible jokes", "scarf",
    "ornament", "sleigh bell", "nutcracker", "snowflake in a jar", "telescope", "hot water bottle", "map to the North Pole",
    "kazoo", "jigsaw puzzle (one piece missing)", "rubber duck in a Santa hat", "music box", "pair of earmuffs",
    "tin of cookies (it's sewing supplies)", "lump of coal (a joke!)", "yo-yo", "board game", "potted mini tree",
    "wooden soldier", "bag of marbles", "snowman kit", "penguin figurine", "fruitcake", "pair of ice skates",
];
const GIFT_TAIL: &[&str] = &[
    "!", " ✦", "… it jingles", " — from the workshop", " (batteries not included)", "", " ♥", ". Just what you wanted!",
    "… with a note: \"open later\"", " wrapped in yesterday's comics", " with a bow on top", "… still warm?",
];
const GIFT_FIXED: &[&str] = &[
    "socks! …again.",
    "a coupon for one hug",
    "it's… another box. ☐",
    "it's empty. the box IS the gift.",
    "a gift card for 1 free nap",
    "an IOU from Santa",
    "a single, perfect snowflake",
    "a sweater with your face on it",
    "it says \"do not open until Dec 25\"",
    "a framed photo of a cat in a hat",
    "a cat. how did that get in there?",
    "glitter. so much glitter.",
    "a smaller present. and inside that…",
    "a voucher for unlimited cocoa",
    "the true meaning of Christmas (and fudge)",
];

const SNOW_FIXED: &[&str] = &[
    "brr… chilly out here!",
    "is it Christmas yet?",
    "nice night for a snowball fight",
    "careful, I'm ticklish",
    "have you seen my carrot? …oh.",
    "I'm feeling a little flaky",
    "warm hugs? …maybe not",
    "the deer never stop to chat",
    "I've been practising my wave",
    "★ merry everything ★",
    "I'm made of 100% recycled snow",
    "don't tell anyone, but I'm melting a bit",
    "my hat was a gift. from a chimney.",
    "the lamp post and I go way back",
    "I'm a snowman of few words. brr.",
    "I tried ice skating once. never again.",
    "who keeps clicking me?",
    "I'm not cold, I'm chill",
    "they call me Frost… just Frost",
    "my buttons are coal. very fashionable.",
    "I watched the sleigh go by. jealous.",
    "have you tried the cocoa at the cabin?",
    "the moon's extra bright tonight",
    "I can't feel my… anything, actually",
    "if you see my scarf's twin, let me know",
    "standing still is my cardio",
    "one day I'll see summer. one day.",
    "a little lower… there. thanks!",
];
const SNOW_FEEL: &[&str] = &["frosty", "festive", "sparkly", "cozy", "a bit wobbly", "extra snowy", "cheerful", "chilly", "magical", "sleepy"];
const SNOW_THING: &[&str] = &["the stars", "the lights", "the cocoa", "the carolers", "the sleigh", "this snow", "my scarf", "the trees", "the moon", "the train"];

const MOON: &[&str] = &[
    "ho ho ho!",
    "on Dasher, on Dancer!",
    "Rudolph, lights on!",
    "one more lap of the sky!",
    "has everyone been good?",
    "next stop: your house!",
    "ho… ho… whoa, low branch!",
    "watch the chimneys, team!",
    "merry Christmas to all!",
    "I see you there! ho ho!",
    "the cookies better be fresh!",
    "full speed ahead, reindeer!",
];


const WISH: &[&str] = &[
    "make a wish!",
    "✦ wish granted ✦",
    "quick, wish!",
    "a wish for snow days",
    "a wish for cozy nights",
    "that one's for you ✦",
    "a wish for cocoa ✦",
    "did you see that one?",
];

const HOUSE: &[&str] = &[
    "someone's baking cookies",
    "the cat knocked over the tree again",
    "family movie night",
    "shh, the kids are asleep",
    "who turned off the lights?",
    "carols playing on the radio",
    "wrapping paper everywhere",
    "grandma's on the phone",
    "the fire's crackling nicely",
    "someone's hiding the presents",
    "board game night: chaos",
    "cocoa's ready!",
];

const HELD: &[&str] = &[
    "whoa! put me down!",
    "wheee!",
    "I can see my house from here!",
    "this is not how sleighs work!",
    "careful, I bruise like a peach",
    "is this the express route?",
    "higher! no wait, lower!",
    "my cocoa! my cocoa!",
    "am I flying?? I'm flying!",
    "excuse me, I was walking there",
];
const KID_HELD: &[&str] = &["wheee!", "again! again!", "I'm a reindeer!", "higher!", "hehehe!", "I can fly!", "best. day. ever."];
const DROPPED: &[&str] = &[
    "…ow. fluffy, but ow.",
    "soft landing!",
    "where am I?",
    "I meant to do that.",
    "the snow broke my fall",
    "everything's spinning…",
    "ten out of ten, would fly again",
    "who keeps doing that?",
];
const POKED: &[&str] = &[
    "oh! hello there!",
    "merry Christmas to you!",
    "lovely evening, isn't it?",
    "did you drop this? …no?",
    "the lights are lovely tonight",
    "happy holidays!",
    "have you seen my mitten?",
    "*waves*",
    "nice cursor you've got",
];
const KID_POKED: &[&str] = &["tag, you're it!", "hehe, that tickles!", "wanna build a snowman?", "I'm on the nice list!", "snowball fight?", "Santa's coming soon!"];
const PET_DEER: &[&str] = &["hello, gentle deer", "what lovely antlers!", "are you one of Santa's?", "soft nose!", "shh, easy now…"];
const PET_FOX: &[&str] = &["hello, little fox!", "what a fluffy tail!", "you look cozy", "aren't you clever", "no stealing cookies!"];
const PET_RABBIT: &[&str] = &["hi, bunny!", "look at those ears!", "so soft!", "hop hop!", "you're adorable"];
const TO_SNOWMAN: &[&str] = &["hi, snowman!", "nice hat, snowman!", "looking sharp, snowman", "stay frosty!", "evening, mister snowman"];
const SNOWMAN_REPLY: &[&str] = &["hello!", "thanks, it's new!", "you too!", "*happy snowman noises*", "brr — I mean, hi!"];
const FOX_GIVES_UP: &[&str] = &["…next time, bunny.", "too fast!", "*pants happily*", "fine, you win", "…was just playing anyway"];

const FOLLOW_YES: &[&str] = &["sure, I'll walk with you!", "lead the way!", "a stroll? lovely", "ooh, an adventure!", "I've got nowhere to be"];
const FOLLOW_KID: &[&str] = &["yay! where are we going?", "I'm coming too!", "can we see the train?"];
const FOLLOW_BYE: &[&str] = &["see you around!", "thanks for the walk!", "merry Christmas!", "that was fun!", "bye for now!"];
const HIT: &[&str] = &["hey!", "oof — right in the scarf!", "oh, it's ON now!", "who threw that?!", "brr, down my collar!", "nice shot…", "my cocoa!", "really? REALLY?"];
const KID_HIT: &[&str] = &["snowball fight!!", "you're gonna get it!", "hehe! again!", "missed m— oh.", "no fair, I wasn't ready!"];
const PICK: &[&str] = &["a handful of berries", "berries! ●", "these look tasty", "for the animals"];

/// Pick lines without repeating any of the last few.
pub struct Lines {
    rng: Rng,
    recent: VecDeque<String>,
}

impl Lines {
    pub fn new(seed: u32) -> Self {
        Lines { rng: Rng::new(seed as u64 ^ 0x11e5), recent: VecDeque::new() }
    }

    fn fresh(&mut self, mut make: impl FnMut(&mut Rng) -> String) -> String {
        let mut s = make(&mut self.rng);
        for _ in 0..12 {
            if !self.recent.contains(&s) {
                break;
            }
            s = make(&mut self.rng);
        }
        self.recent.push_back(s.clone());
        if self.recent.len() > 24 {
            self.recent.pop_front();
        }
        s
    }

    pub fn gift(&mut self) -> String {
        self.fresh(|r| {
            if r.chance(0.25) {
                r.pick(GIFT_FIXED).to_string()
            } else {
                format!("{} {}{}", r.pick(GIFT_ADJ), r.pick(GIFT_ITEM), r.pick(GIFT_TAIL))
            }
        })
    }

    pub fn snowman(&mut self) -> String {
        self.fresh(|r| match r.int(0, 10) {
            0 => format!("feeling {} tonight", r.pick(SNOW_FEEL)),
            1 => format!("have you seen {}? {}!", r.pick(SNOW_THING), r.pick(&["gorgeous", "so festive", "wow", "magical", "lovely"])),
            2 => format!("I've been standing here {} hours. {}", r.int(2, 900), r.pick(&["worth it", "no regrets", "send cocoa", "still smiling"])),
            _ => r.pick(SNOW_FIXED).to_string(),
        })
    }

    pub fn moon(&mut self) -> String {
        self.fresh(|r| r.pick(MOON).to_string())
    }


    pub fn wish(&mut self) -> String {
        self.fresh(|r| r.pick(WISH).to_string())
    }

    /// What a person says while being carried around.
    pub fn held(&mut self, kid: bool) -> String {
        self.fresh(|r| r.pick(if kid { KID_HELD } else { HELD }).to_string())
    }

    pub fn dropped(&mut self) -> String {
        self.fresh(|r| r.pick(DROPPED).to_string())
    }

    pub fn poked(&mut self, kid: bool) -> String {
        self.fresh(|r| r.pick(if kid { KID_POKED } else { POKED }).to_string())
    }

    /// Someone petting an animal: 0 deer, 1 fox, 2 rabbit.
    pub fn pet(&mut self, animal: u8) -> String {
        self.fresh(|r| r.pick(match animal {
            0 => PET_DEER,
            1 => PET_FOX,
            _ => PET_RABBIT,
        }).to_string())
    }


    pub fn to_snowman(&mut self) -> (String, String) {
        let a = self.fresh(|r| r.pick(TO_SNOWMAN).to_string());
        let b = SNOWMAN_REPLY[self.rng.int(0, SNOWMAN_REPLY.len() as i32) as usize].to_string();
        (a, b)
    }

    pub fn fox_gives_up(&mut self) -> String {
        self.fresh(|r| r.pick(FOX_GIVES_UP).to_string())
    }



    pub fn follow_yes(&mut self, kid: bool) -> String {
        self.fresh(|r| r.pick(if kid { FOLLOW_KID } else { FOLLOW_YES }).to_string())
    }

    pub fn follow_bye(&mut self) -> String {
        self.fresh(|r| r.pick(FOLLOW_BYE).to_string())
    }

    /// Someone noticing a light show's about to start.
    pub fn show_soon(&mut self) -> String {
        self.fresh(|r| r.pick(&["ooh, the light show's starting!", "everyone, to the big tree!", "quick, the lights!", "light show! come on!", "gather round, gather round!"]).to_string())
    }

    /// Hit by the player's snowball.
    pub fn hit(&mut self, kid: bool) -> String {
        self.fresh(|r| r.pick(if kid { KID_HIT } else { HIT }).to_string())
    }

    pub fn pick(&mut self) -> String {
        self.fresh(|r| r.pick(PICK).to_string())
    }

    pub fn house(&mut self) -> String {
        self.fresh(|r| r.pick(HOUSE).to_string())
    }
}
