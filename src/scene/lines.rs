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

const WALKER: &[&str] = &[
    "merry Christmas!",
    "evening!",
    "lovely night for a walk",
    "happy holidays!",
    "mind the ice!",
    "is that a sleigh?",
    "cocoa time soon",
    "the tree looks amazing this year",
    "brr! should've worn gloves",
    "season's greetings!",
    "did you see the lights?",
    "off to the carol service",
    "just picking up some pies",
    "the snow's coming down nicely",
    "ho ho—oh, wrong line",
    "have a cozy night!",
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
const GREET_A: &[&str] = &["merry Christmas!", "evening, neighbour!", "lovely snow tonight!", "happy holidays!", "off to the carols?", "cold enough for you?"];
const GREET_B: &[&str] = &["and to you!", "evening!", "isn't it just?", "you too!", "wouldn't miss them!", "brr, just about!"];
const TO_SNOWMAN: &[&str] = &["hi, snowman!", "nice hat, snowman!", "looking sharp, snowman", "stay frosty!", "evening, mister snowman"];
const SNOWMAN_REPLY: &[&str] = &["hello!", "thanks, it's new!", "you too!", "*happy snowman noises*", "brr — I mean, hi!"];
const FOX_GIVES_UP: &[&str] = &["…next time, bunny.", "too fast!", "*pants happily*", "fine, you win", "…was just playing anyway"];

// ---- conversations: little scripted exchanges, speakers alternate
// (A, B, A, B…; a third speaker, when there is one, takes every third line)
const CHAT2: &[&[&str]] = &[
    &["did you see the lights on the church?", "every year they add more!", "one day it'll be visible from space", "ha! one day."],
    &["finished your shopping yet?", "I haven't even started…", "there's still time!", "is there though?"],
    &["smell that? gingerbread.", "it's coming from the bakery", "we should go before it's gone", "race you!"],
    &["the snow's perfect for snowmen", "too powdery, if you ask me", "you're no fun", "I'm extremely fun."],
    &["how's your mum?", "making enough stuffing for an army", "so, the usual", "the usual!"],
    &["I heard sleigh bells last night", "probably the wind…", "the wind doesn't go ho ho ho", "…fair point"],
    &["are you going to the carols?", "only if there's cocoa after", "there's always cocoa after", "then I'm in!"],
    &["I wrapped everything in comics", "that's very eco of you", "…I ran out of paper", "still counts!"],
    &["the train's late again", "it's the snow on the tracks", "it's always the snow", "it IS a lot of snow"],
    &["who built that snowman?", "nobody knows", "it just… appeared?", "best not to ask"],
    &["I tried to knit a scarf", "and?", "it's a very long sock now", "socks are good!"],
    &["is it me or is it colder tonight?", "it's definitely colder", "I can't feel my nose", "it's still there, don't worry"],
    &["have you seen the fox around?", "it stole my mitten yesterday", "a fashionable fox", "a menace, more like"],
    &["what are you asking Santa for?", "a quiet morning", "that's not a toy", "I'm not seven anymore"],
    &["the big tree looks taller this year", "they say it grows when no one's looking", "trees do that, yes", "spookily, though"],
    &["do you think it'll snow on Christmas?", "it hasn't stopped snowing since November", "so… yes?", "so, yes."],
    &["I've been practising my carols", "let's hear one!", "♪ deck the… ♪ no, I'm shy", "next year then"],
    &["the bakery's out of mince pies", "a tragedy", "they're making more at nine", "I'll be first in line"],
    &["look at the stars tonight", "you can see the whole Milky Way", "and the aurora!", "best night of the year"],
    &["my cat climbed the tree again", "with the ornaments on?", "she IS an ornament now", "festive cat!"],
];
const CHAT3: &[&[&str]] = &[
    &["who's up for a snowball fight?", "always!", "only if I'm on the winning side", "there are no sides, just snow"],
    &["cocoa at mine after this?", "with marshmallows?", "obviously with marshmallows", "then it's a plan!"],
    &["did everyone get the card I sent?", "the one with glitter?", "I'm still finding glitter", "glitter is forever"],
    &["shall we go see the big tree?", "lead the way!", "I'll catch up, my boots are slow", "we'll wait for you!"],
];
const KID_CHAT: &[&[&str]] = &[
    &["I'm gonna stay up and see Santa!", "you fell asleep at eight last year", "that was a TRAINING year", "oh, okay"],
    &["race you to the tree!", "no fair, you started already!", "hehe!", "wait for meee!"],
    &["I made a snow angel!", "where?", "…a deer walked on it", "it's a deer angel now"],
    &["what's your favourite reindeer?", "Rudolph, obviously", "I like Blitzen", "Blitzen's cool too"],
];
// the player character's side of things
const YOU_HI: &[&str] = &["hello there!", "merry Christmas!", "evening!", "lovely night, isn't it?", "happy holidays!", "how's it going?", "nice hat!"];
const THEY_REPLY: &[&str] = &[
    "oh, hello! merry Christmas!",
    "lovely to see you out and about",
    "evening! mind the icy bit by the lamp",
    "have you seen the big tree? gorgeous",
    "happy holidays to you too!",
    "off to the bakery — want anything?",
    "brr, isn't it chilly!",
    "you look cozy in that hat",
    "the church bells are ringing soon",
    "can't stop, cocoa's getting cold!",
];
const KID_REPLY: &[&str] = &["hi!! wanna build a snowman?", "I saw a fox!", "is it Christmas yet?", "tag! you're it!", "I'm on the nice list. probably.", "do you like my hat?"];
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

    pub fn walker(&mut self) -> String {
        self.fresh(|r| r.pick(WALKER).to_string())
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

    /// Two passers-by greeting each other: (first, reply).
    pub fn greet_pair(&mut self) -> (String, String) {
        let i = self.rng.int(0, GREET_A.len() as i32) as usize;
        (GREET_A[i].to_string(), GREET_B[i.min(GREET_B.len() - 1)].to_string())
    }

    pub fn to_snowman(&mut self) -> (String, String) {
        let a = self.fresh(|r| r.pick(TO_SNOWMAN).to_string());
        let b = SNOWMAN_REPLY[self.rng.int(0, SNOWMAN_REPLY.len() as i32) as usize].to_string();
        (a, b)
    }

    pub fn fox_gives_up(&mut self) -> String {
        self.fresh(|r| r.pick(FOX_GIVES_UP).to_string())
    }

    /// A little exchange for `n` (2 or 3) people; kids talk like kids.
    /// Returns the lines in order; speaker k says line k (mod n).
    pub fn conversation(&mut self, n: usize, kids: bool) -> Vec<String> {
        let pool: &[&[&str]] = if kids { KID_CHAT } else if n >= 3 { CHAT3 } else { CHAT2 };
        let i = self.rng.int(0, pool.len() as i32) as usize;
        // avoid replaying the last few conversations
        let mut idx = i;
        for k in 0..pool.len() {
            let j = (i + k) % pool.len();
            if !self.recent.contains(&pool[j][0].to_string()) {
                idx = j;
                break;
            }
        }
        let lines: Vec<String> = pool[idx].iter().map(|s| s.to_string()).collect();
        self.recent.push_back(lines[0].clone());
        if self.recent.len() > 24 {
            self.recent.pop_front();
        }
        lines
    }

    /// The player says hi; (the player's line, the reply).
    pub fn you_hi(&mut self, kid: bool) -> (String, String) {
        let a = self.fresh(|r| r.pick(YOU_HI).to_string());
        let b = self.fresh(|r| r.pick(if kid { KID_REPLY } else { THEY_REPLY }).to_string());
        (a, b)
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
