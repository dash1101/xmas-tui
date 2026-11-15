//! What the townsfolk say, to each other and to the player. Everyone has a
//! name; conversations are written exchanges (each line answers the one
//! before), chosen to fit what's actually going on — the time of day, heavy
//! snow, a train that just came in, a light show, the blackout, Santa
//! overhead, how close Christmas is, and where they're standing. A
//! passer-by can stop and chime in, and the group answers them.

use crate::rng::Rng;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Part {
    #[default]
    Night,
    Morning,
    Day,
    Evening,
}

/// What's going on, as far as conversation is concerned.
#[derive(Clone, Copy, Debug, Default)]
pub struct TalkCtx {
    pub part: Part,
    /// A blizzard or strong gusts.
    pub snowy: bool,
    /// A train stopped at the station a moment ago.
    pub train: bool,
    /// A light show is on, or just was.
    pub show: bool,
    /// The power's out, or just came back.
    pub blackout: bool,
    /// Santa just flew over.
    pub sleigh: bool,
    /// A shooting star, comet or fireworks a moment ago.
    pub stars: bool,
    /// Days until Christmas (0 on the day).
    pub days: i64,
    /// Landmark columns: the big tree, the church, the cocoa cabin, the
    /// station (where known).
    pub tree: Option<f32>,
    pub church: Option<f32>,
    pub shop: Option<f32>,
    pub station: Option<f32>,
}

impl TalkCtx {
    fn near(&self, at: Option<f32>, x: f32) -> bool {
        at.map(|a| (a - x).abs() < 30.0).unwrap_or(false)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum When {
    Any,
    Night,
    Morning,
    Day,
    Evening,
    Snowy,
    Train,
    Show,
    Blackout,
    Sleigh,
    Stars,
    Soon,
    Eve,
    Xmas,
    Tree,
    Church,
    Shop,
    Station,
}

impl When {
    fn ok(self, c: &TalkCtx, x: f32) -> bool {
        match self {
            When::Any => true,
            When::Night => c.part == Part::Night,
            When::Morning => c.part == Part::Morning,
            When::Day => c.part == Part::Day,
            When::Evening => c.part == Part::Evening,
            When::Snowy => c.snowy,
            When::Train => c.train,
            When::Show => c.show,
            When::Blackout => c.blackout,
            When::Sleigh => c.sleigh,
            When::Stars => c.stars,
            When::Soon => (2..=7).contains(&c.days),
            When::Eve => c.days == 1,
            When::Xmas => c.days == 0,
            When::Tree => c.near(c.tree, x),
            When::Church => c.near(c.church, x),
            When::Shop => c.near(c.shop, x),
            When::Station => c.near(c.station, x),
        }
    }
    /// Topical talk is more likely than small talk.
    fn weight(self) -> u32 {
        if self == When::Any {
            1
        } else {
            4
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Who {
    Adults,
    Kids,
    /// An adult (A) and a kid (B).
    Mixed,
    Three,
}

struct Script {
    when: When,
    who: Who,
    /// (speaker 0 = A, 1 = B, 2 = C; line). {a} {b} {c}: names, {days}.
    lines: &'static [(u8, &'static str)],
}

const fn s(when: When, who: Who, lines: &'static [(u8, &'static str)]) -> Script {
    Script { when, who, lines }
}

use When::*;
use Who::*;

const SCRIPTS: &[Script] = &[
    // ---- small talk, any time
    s(Any, Adults, &[(0, "{b}! merry Christmas!"), (1, "and to you, {a}! ready for the big day?"), (0, "almost. just the wrapping left"), (1, "ha, I always leave that till last too")]),
    s(Any, Adults, &[(0, "how's the family, {b}?"), (1, "all home for the holidays. house is full!"), (0, "full houses are the best kind"), (1, "tell that to my sofa")]),
    s(Any, Adults, &[(0, "finished the shopping, {b}?"), (1, "one present left. for my brother"), (0, "what do you get the man who has everything?"), (1, "socks. you always get him socks")]),
    s(Any, Adults, &[(0, "going to the carols on Sunday?"), (1, "wouldn't miss them. are you singing?"), (0, "only if I can stand at the back"), (1, "we'll find you a spot behind the tenors")]),
    s(Any, Adults, &[(0, "{b}, tried the cocoa at the cabin yet?"), (1, "twice today already"), (0, "is it really that good?"), (1, "the marshmallows are the size of my fist")]),
    s(Any, Adults, &[(0, "my cat climbed the tree again last night"), (1, "oh no — the ornaments?"), (0, "all fine. she's an ornament now"), (1, "a very festive cat")]),
    s(Any, Adults, &[(0, "I tried to knit a scarf this year"), (1, "how did it turn out?"), (0, "it's a very long sock now"), (1, "well, socks are always useful")]),
    s(Any, Adults, &[(0, "who built that snowman, {b}?"), (1, "nobody knows. it was just there one morning"), (0, "I think it waved at me yesterday"), (1, "…let's not talk about that")]),
    s(Any, Adults, &[(0, "the bakery's out of mince pies again"), (1, "already? I only wanted two"), (0, "they're baking more at nine"), (1, "then I'll be first in line")]),
    s(Any, Adults, &[(0, "what are you hoping Santa brings, {b}?"), (1, "a quiet morning and a hot cup of tea"), (0, "that's not a toy, {b}"), (1, "I'm not seven anymore, {a}")]),
    s(Any, Adults, &[(0, "are you hosting this year, {b}?"), (1, "twelve people and one oven"), (0, "brave. very brave"), (1, "or very foolish. we'll see")]),
    s(Any, Adults, &[(0, "I hung the lights myself this year"), (1, "they look wonderful from here"), (0, "only fell off the ladder twice"), (1, "worth it, {a}. truly")]),
    // ---- the time of day
    s(Morning, Adults, &[(0, "morning, {b}! early start?"), (1, "the dog wanted a walk in the snow"), (0, "and where is he now?"), (1, "…that's a very good question")]),
    s(Morning, Adults, &[(0, "lovely bright morning"), (1, "the snow sparkles when the sun's low"), (0, "makes the cold almost worth it"), (1, "almost, {a}. almost")]),
    s(Day, Adults, &[(0, "such a clear day, {b}"), (1, "you can see the cabins up the mountain"), (0, "I'd love to spend a winter up there"), (1, "no bakery, though")]),
    s(Day, Adults, &[(0, "the sun's actually warm today"), (1, "the icicles are dripping on my porch"), (0, "don't let the snowman hear"), (1, "ha! he'll be fine in the shade")]),
    s(Evening, Adults, &[(0, "evening, {b}. heading home?"), (1, "just as the lights come on — my favourite time"), (0, "the whole village glows"), (1, "I could watch it for hours")]),
    s(Evening, Adults, &[(0, "look, the sky's going pink"), (1, "sunset already? the days are so short"), (0, "more time for lights, though"), (1, "and cocoa. always more cocoa")]),
    s(Night, Adults, &[(0, "the stars are out tonight, {b}"), (1, "you can see the aurora, too"), (0, "it looks like ribbons"), (1, "Christmas ribbons for the whole sky")]),
    s(Night, Adults, &[(0, "still out this late, {b}?"), (1, "couldn't sleep. the snow's too pretty"), (0, "same. the quiet's the best part"), (1, "shh — listen. not a sound")]),
    // ---- weather and goings-on
    s(Snowy, Adults, &[(0, "it's really coming down now!"), (1, "we'll be digging out the door tomorrow"), (0, "the kids will be thrilled"), (1, "the kids don't do the digging")]),
    s(Snowy, Adults, &[(0, "hold onto your hat, {b}!"), (1, "too late — it's halfway to the church"), (0, "I'll help you look"), (1, "thanks. it was a gift, too")]),
    s(Train, Adults, &[(0, "did you see the train come in?"), (1, "packed with visitors for the holidays"), (0, "the station's never been so busy"), (1, "good for the cocoa cabin, though")]),
    s(Train, Adults, &[(0, "was that the train from the city?"), (1, "right on time, for once"), (0, "must be the Christmas spirit"), (1, "or a new driver")]),
    s(Show, Adults, &[(0, "did you see the light show, {b}?"), (1, "I've never seen the tree so bright"), (0, "the rainbow bit at the end!"), (1, "every year they outdo themselves")]),
    s(Show, Adults, &[(0, "the lights were dancing!"), (1, "I think the church was in on it too"), (0, "the whole village, all at once"), (1, "best part of December")]),
    s(Blackout, Adults, &[(0, "the power went out! did you see?"), (1, "something sparked up by the powerline"), (0, "who'd be poking at a powerline?"), (1, "…someone with too much free time")]),
    s(Blackout, Adults, &[(0, "every light went dark at once"), (1, "I lit every candle in the house"), (0, "very cozy, actually"), (1, "we should do it on purpose next year")]),
    s(Stars, Adults, &[(0, "did you see that shooting star, {b}?"), (1, "I did! I made a wish"), (0, "what did you wish for?"), (1, "if I tell you, it won't come true")]),
    s(Stars, Kids, &[(0, "I saw a shooting star!"), (1, "me too! what did you wish?"), (0, "a puppy. and snow forever"), (1, "I wished for the same puppy")]),
    s(Sleigh, Adults, &[(0, "{b}! did you see that, past the moon?"), (1, "sleigh bells… was that…?"), (0, "it had reindeer, I'm sure of it"), (1, "then we'd better be on the nice list")]),
    s(Soon, Adults, &[(0, "only {days} days to go, {b}!"), (1, "don't remind me — nothing's wrapped"), (0, "I'll lend you my tape"), (1, "you're a lifesaver, {a}")]),
    s(Eve, Adults, &[(0, "Christmas Eve at last!"), (1, "the kids will never sleep tonight"), (0, "neither will I, honestly"), (1, "keep an eye on the sky later")]),
    s(Xmas, Adults, &[(0, "merry Christmas, {b}!"), (1, "merry Christmas! did Santa find you?"), (0, "he left me a scarf. a very long scarf"), (1, "…was it a sock?")]),
    // ---- where they are
    s(Tree, Adults, &[(0, "the big tree looks taller this year"), (1, "they say it grows when nobody's looking"), (0, "trees do that, {b}"), (1, "not this fast, they don't")]),
    s(Tree, Adults, &[(0, "how many lights on it, do you think?"), (1, "I counted two hundred, then lost track"), (0, "there's one star on top, at least"), (1, "that I can count")]),
    s(Church, Adults, &[(0, "the bells will ring on the hour"), (1, "I set my clock by them"), (0, "is that why you're always late?"), (1, "…they must be slow")]),
    s(Shop, Adults, &[(0, "cocoa, {b}? my treat"), (1, "you've twisted my arm"), (0, "with marshmallows?"), (1, "is there any other way?")]),
    s(Station, Adults, &[(0, "waiting for someone, {b}?"), (1, "my sister, on the next train"), (0, "how long since you've seen her?"), (1, "since last Christmas. I can't wait")]),
    // ---- three of them
    s(Any, Three, &[(0, "who's up for a snowball fight?"), (1, "always!"), (2, "only if I'm on the winning side"), (0, "there are no sides, {c}. only snow")]),
    s(Any, Three, &[(0, "cocoa at mine after this?"), (1, "with marshmallows?"), (2, "obviously with marshmallows"), (0, "then it's a plan")]),
    s(Any, Three, &[(0, "shall we go see the big tree?"), (1, "lead the way, {a}!"), (2, "I'll catch up — my boots are slow"), (1, "we'll wait for you, {c}")]),
    s(Evening, Three, &[(0, "look how the windows glow"), (1, "like the whole village is smiling"), (2, "that's very poetic, {b}"), (1, "it's the cocoa talking")]),
    s(Snowy, Three, &[(0, "we should head inside, it's getting wild"), (1, "one more lap of the square"), (2, "{b}, my ears are frozen"), (1, "fine, fine. inside it is")]),
    s(Train, Three, &[(0, "did the train bring your cousins?"), (1, "all six of them"), (2, "six?! where will they sleep?"), (1, "I was hoping you'd ask, {c}")]),
    // ---- kids
    s(Any, Kids, &[(0, "race you to the tree, {b}!"), (1, "no fair, you started already!"), (0, "hehe!"), (1, "wait for meee!")]),
    s(Any, Kids, &[(0, "I'm gonna stay up and see Santa!"), (1, "you fell asleep at eight last year"), (0, "that was a TRAINING year"), (1, "oh. okay")]),
    s(Any, Kids, &[(0, "I made a snow angel!"), (1, "where?"), (0, "…a deer walked on it"), (1, "it's a deer angel now")]),
    s(Any, Kids, &[(0, "what's your favourite reindeer, {b}?"), (1, "Rudolph, obviously"), (0, "I like Blitzen"), (1, "Blitzen's cool too")]),
    s(Soon, Kids, &[(0, "{days} more sleeps, {b}!"), (1, "I've been counting since November"), (0, "I've been counting since July"), (1, "whoa")]),
    s(Train, Kids, &[(0, "did you see the train?!"), (1, "it went CHOO CHOO"), (0, "when I grow up I'm gonna drive it"), (1, "can I be the whistle?")]),
    s(Show, Kids, &[(0, "the tree turned RAINBOW"), (1, "I saw! I saw!"), (0, "do you think it'll do it again?"), (1, "I'm not blinking till it does")]),
    s(Snowy, Kids, &[(0, "snow day! snow day!"), (1, "let's build a snow fort"), (0, "with a door and windows"), (1, "and a cocoa stand!")]),
    // ---- a grown-up and a kid
    s(Any, Mixed, &[(0, "hello there, {b}! being good?"), (1, "SO good. ask anyone"), (0, "I'll put in a word with Santa"), (1, "thank you!!")]),
    s(Any, Mixed, &[(0, "mind the ice, {b}!"), (1, "I'm sliding on purpose!"), (0, "of course you are"), (1, "wheee!")]),
    s(Soon, Mixed, &[(0, "what's on your list, {b}?"), (1, "a sled. a red one. a FAST one"), (0, "a fast red sled. noted"), (1, "you'll tell Santa?")]),
];

/// A passer-by joining someone else's conversation, and the group's reply.
const CHIMES: &[(When, &str, &str)] = &[
    (Any, "what are you two plotting?", "nothing, {c}! …mostly"),
    (Any, "mind if I join? it's freezing", "the more the merrier, {c}"),
    (Any, "{a}! {b}! merry Christmas!", "merry Christmas, {c}!"),
    (Any, "is that cocoa I smell?", "from the cabin, {c}. go on!"),
    (Train, "talking about the train? it was packed!", "we were! did your visitors make it?"),
    (Show, "wasn't the light show something?", "we were just saying!"),
    (Snowy, "you'll freeze standing out here!", "we're going, {c}, we're going"),
    (Blackout, "was it you two who cut the power?", "us? never!"),
    (Night, "still up, you two?", "the stars kept us out"),
    (Sleigh, "did you hear the sleigh bells too?", "we did! we thought we'd dreamt it"),
    (Stars, "did you two see the shooting star?", "we did! we both made a wish"),
];

/// Things said in passing (to nobody in particular).
const REMARKS: &[(When, bool, &str)] = &[
    (Any, false, "merry Christmas, everyone!"),
    (Any, false, "mind the icy patch by the lamp"),
    (Any, false, "smells like gingerbread around here"),
    (Any, false, "I love this time of year"),
    (Any, false, "nearly time for cocoa"),
    (Morning, false, "good morning, village!"),
    (Morning, false, "fresh snow overnight!"),
    (Day, false, "what a bright winter's day"),
    (Evening, false, "the lights are coming on"),
    (Evening, false, "evening, everyone!"),
    (Night, false, "what a starry night"),
    (Night, false, "everyone's asleep… except me"),
    (Snowy, false, "brr! it's a blizzard!"),
    (Snowy, false, "I can't see my own boots"),
    (Train, false, "a train full of visitors!"),
    (Train, false, "the station's busy today"),
    (Show, false, "look at the tree go!"),
    (Blackout, false, "who turned out the lights?"),
    (Sleigh, false, "was that a sleigh?!"),
    (Soon, false, "only {days} days to go!"),
    (Eve, false, "Christmas Eve! eyes on the sky tonight"),
    (Xmas, false, "merry Christmas!!"),
    (Any, true, "wheee!"),
    (Any, true, "I'm gonna build a snowman"),
    (Any, true, "tag! you're it!"),
    (Snowy, true, "snow day!!"),
    (Show, true, "RAINBOW TREE!"),
    (Train, true, "choo choo!"),
    (Soon, true, "{days} more sleeps!"),
];

/// Two people passing each other: (A says, B answers).
const GREETS: &[(When, &str, &str)] = &[
    (Morning, "morning, {b}!", "morning, {a}! sleep well?"),
    (Day, "afternoon, {b}!", "hello, {a}! lovely day"),
    (Evening, "evening, {b}!", "evening, {a}! off home?"),
    (Night, "still out, {b}?", "couldn't resist the stars, {a}"),
    (Snowy, "{b}! wrap up warm!", "you too, {a}!"),
    (Any, "merry Christmas, {b}!", "and to you, {a}!"),
];

/// Chatting with the player: each E is one exchange (player, them).
const CHATS: &[(When, bool, &[(&str, &str)])] = &[
    (Any, false, &[("lovely village, isn't it?", "forty winters I've lived here. never gets old"), ("forty!", "and forty snowmen, give or take"), ("any tips for a newcomer?", "the cocoa cabin. trust me")]),
    (Any, false, &[("what's your favourite thing about Christmas?", "the quiet morning before everyone wakes"), ("that sounds lovely", "and the cookies. mostly the cookies"), ("fair enough", "come by for one sometime")]),
    (Snowy, false, &[("quite the snow today!", "best weather there is, if you ask me"), ("isn't it cold?", "that's what cocoa's for"), ("see you by the fire, then", "save me a seat!")]),
    (Train, false, &[("did you see the train come in?", "my grandson was on it!"), ("how lovely", "he's off building a snowman already"), ("hope it's a big one", "with his energy? it'll be huge")]),
    (Show, false, &[("that light show!", "every year they outdo themselves"), ("who runs it?", "the snowman, some say. I don't ask"), ("I'll keep an eye on him", "wise choice")]),
    (Night, false, &[("still out this late?", "I always walk under the stars before bed"), ("they're bright tonight", "you can even see the aurora"), ("good night, then", "good night! stay warm")]),
    (Evening, false, &[("the lights are coming on", "my favourite moment of the day"), ("it's like magic", "it IS magic, this time of year"), ("I believe it", "that's the spirit")]),
    (Soon, false, &[("only {days} days to go!", "and I haven't wrapped a thing"), ("need a hand?", "only if you can wrap a bicycle"), ("…maybe not", "ha! didn't think so")]),
    (Sleigh, false, &[("did you see the sleigh?!", "I did! I waved and everything"), ("do you think he saw you?", "he jingled, so I'll say yes"), ("then you're on the list", "I've been very good this year")]),
    (Stars, false, &[("did you see that shooting star?", "I did. I made a wish on it"), ("what for?", "a white Christmas. looks like it worked")]),
    (Blackout, false, &[("the power's out!", "I heard something sparked by the hills"), ("spooky", "cozy, I'd say. candles everywhere"), ("enjoy the candlelight", "I will!")]),
    (Any, true, &[("hi! what are you up to?", "making a list for Santa!"), ("what's on it?", "a sled, a puppy and a dinosaur"), ("a real dinosaur?", "a SMALL one")]),
    (Snowy, true, &[("snowball fight?", "you're ON!"), ("on three?", "THREE!"), ("hey!", "hehehe!")]),
    (Train, true, &[("did you see the train?", "I waved and the driver waved back!"), ("lucky you", "best. day. ever.")]),
];

/// Something happening that people notice and react to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ev {
    ShootingStar,
    Comet,
    Sleigh,
    Fireworks,
    Blackout,
    PowerBack,
    LightShow,
    Aurora,
    Blizzard,
    Train,
    /// A typed easter egg (light show, snow burst, cheat code…).
    Magic,
}

/// (event, kid?, reaction)
const REACT: &[(Ev, bool, &str)] = &[
    (Ev::ShootingStar, false, "look — a shooting star!"),
    (Ev::ShootingStar, false, "quick, make a wish!"),
    (Ev::ShootingStar, false, "did you see that streak?"),
    (Ev::ShootingStar, true, "SHOOTING STAR! I wished for a puppy!"),
    (Ev::ShootingStar, true, "wish! wish! wish!"),
    (Ev::Comet, false, "a comet! I've never seen one"),
    (Ev::Comet, false, "look at the tail on that"),
    (Ev::Comet, true, "is that a SPACESHIP?!"),
    (Ev::Sleigh, false, "is that… Santa?!"),
    (Ev::Sleigh, false, "sleigh bells! up there!"),
    (Ev::Sleigh, false, "I'd know those reindeer anywhere"),
    (Ev::Sleigh, true, "SANTA!! SANTA, OVER HERE!"),
    (Ev::Sleigh, true, "he's checking his list!"),
    (Ev::Fireworks, false, "fireworks! how lovely"),
    (Ev::Fireworks, false, "ooh — that one was gold!"),
    (Ev::Fireworks, true, "BOOM! do another one!"),
    (Ev::Blackout, false, "who turned out the lights?!"),
    (Ev::Blackout, false, "the power's gone!"),
    (Ev::Blackout, false, "something sparked up on the hill"),
    (Ev::Blackout, true, "it's DARK! cool!"),
    (Ev::Blackout, true, "is it a ghost?!"),
    (Ev::PowerBack, false, "ah, the lights are back"),
    (Ev::PowerBack, false, "power's back on! hurrah"),
    (Ev::PowerBack, true, "awww, it was fun in the dark"),
    (Ev::LightShow, false, "the light show's starting!"),
    (Ev::LightShow, false, "look at the tree!"),
    (Ev::LightShow, true, "RAINBOW!!"),
    (Ev::Aurora, false, "the aurora's flaring up!"),
    (Ev::Aurora, false, "look at the sky — it's glowing green"),
    (Ev::Aurora, true, "the sky's dancing!"),
    (Ev::Blizzard, false, "here comes a blizzard!"),
    (Ev::Blizzard, false, "hold onto your hats!"),
    (Ev::Blizzard, true, "SNOWSTORM!!"),
    (Ev::Train, false, "here comes the train"),
    (Ev::Train, false, "right on time"),
    (Ev::Train, true, "TRAIN! choo choo!"),
    (Ev::Magic, false, "what on earth was that?"),
    (Ev::Magic, false, "did anyone else see that?!"),
    (Ev::Magic, false, "I think the village is enchanted"),
    (Ev::Magic, true, "MAGIC! do it again!"),
];

impl Talk {
    /// Someone reacting to `ev`.
    pub fn react(&mut self, ev: Ev, kid: bool) -> String {
        let cands: Vec<&str> = REACT.iter().filter(|r| r.0 == ev && r.1 == kid).map(|r| r.2).collect();
        let cands = if cands.is_empty() { REACT.iter().filter(|r| r.0 == ev).map(|r| r.2).collect() } else { cands };
        (*self.rng.pick(&cands)).to_string()
    }
}

const ADULT_NAMES: &[&str] = &[
    "Martha", "Henry", "Clara", "Walter", "Edith", "Arthur", "Rosa", "Felix", "Ingrid", "Oscar", "June", "Theo", "Agnes", "Hugo", "Nora", "Silas", "Mabel",
    "Otto", "Greta", "Albert", "Ivy", "Rupert", "Elsie", "Bertie",
];
const KID_NAMES: &[&str] = &["Pip", "Tilly", "Benji", "Rosie", "Ned", "Lulu", "Finn", "Hattie", "Milo", "Dot", "Alfie", "Poppy"];

pub fn name(rng: &mut Rng, kid: bool) -> &'static str {
    *rng.pick(if kid { KID_NAMES } else { ADULT_NAMES })
}

fn fill(s: &str, names: &[&str], days: i64) -> String {
    let get = |i: usize| names.get(i).copied().unwrap_or("friend");
    s.replace("{a}", get(0)).replace("{b}", get(1)).replace("{c}", get(2)).replace("{days}", &days.max(0).to_string())
}

/// Picks things to say without repeating what was just said.
pub struct Talk {
    rng: Rng,
    recent: VecDeque<usize>,
}

impl Talk {
    pub fn new(seed: u32) -> Self {
        Talk { rng: Rng::new(seed as u64 ^ 0x7a1c), recent: VecDeque::new() }
    }

    fn pick_weighted<T>(&mut self, items: &[(usize, T, u32)]) -> Option<usize> {
        let total: u32 = items.iter().map(|i| i.2).sum();
        if total == 0 {
            return None;
        }
        let mut r = self.rng.int(0, total as i32) as u32;
        for (k, it) in items.iter().enumerate() {
            if r < it.2 {
                return Some(k);
            }
            r -= it.2;
        }
        Some(items.len() - 1)
    }

    fn remember(&mut self, key: usize) {
        self.recent.push_back(key);
        if self.recent.len() > 16 {
            self.recent.pop_front();
        }
    }

    /// A conversation for these speakers (names; `kids[i]`: speaker i is a
    /// child) at column `x`. Returns (speaker index, line) in order.
    pub fn conversation(&mut self, ctx: &TalkCtx, x: f32, names: &[&str], kids: &[bool]) -> Vec<(usize, String)> {
        let n = names.len().min(3);
        let all_kids = kids.iter().take(n).all(|&k| k);
        let any_kid = kids.iter().take(n).any(|&k| k);
        let mut cands = vec![];
        for (i, sc) in SCRIPTS.iter().enumerate() {
            let fits = match sc.who {
                Three => n == 3 && !any_kid,
                Adults => n == 2 && !any_kid,
                Kids => n == 2 && all_kids,
                Mixed => n == 2 && !kids[0] && kids.get(1) == Some(&true),
            };
            if fits && sc.when.ok(ctx, x) && !self.recent.contains(&i) {
                cands.push((i, (), sc.when.weight()));
            }
        }
        // a kid and a grown-up the other way round: swap so the grown-up leads
        let mut order: Vec<usize> = (0..n).collect();
        if cands.is_empty() && n == 2 && kids[0] && !kids[1] {
            order.swap(0, 1);
            return self.conversation(ctx, x, &[names[1], names[0]], &[false, true]).into_iter().map(|(s, l)| (order[s], l)).collect();
        }
        let Some(k) = self.pick_weighted(&cands) else { return vec![] };
        let id = cands[k].0;
        self.remember(id);
        let names: Vec<&str> = order.iter().map(|&i| names[i]).collect();
        SCRIPTS[id].lines.iter().map(|&(sp, l)| ((sp as usize).min(n - 1), fill(l, &names, ctx.days))).collect()
    }

    /// A passer-by (`c`) joins people talking (`a`, `b`): (their line, the reply).
    pub fn chime(&mut self, ctx: &TalkCtx, x: f32, a: &str, b: &str, c: &str) -> (String, String) {
        let cands: Vec<(usize, (), u32)> = CHIMES.iter().enumerate().filter(|(_, ch)| ch.0.ok(ctx, x)).map(|(i, ch)| (i, (), ch.0.weight())).collect();
        let k = self.pick_weighted(&cands).map(|k| cands[k].0).unwrap_or(0);
        let names = [a, b, c];
        (fill(CHIMES[k].1, &names, ctx.days), fill(CHIMES[k].2, &names, ctx.days))
    }

    /// Something said in passing.
    pub fn remark(&mut self, ctx: &TalkCtx, x: f32, kid: bool) -> String {
        let cands: Vec<(usize, (), u32)> = REMARKS
            .iter()
            .enumerate()
            .filter(|(i, r)| r.1 == kid && r.0.ok(ctx, x) && !self.recent.contains(&(1000 + i)))
            .map(|(i, r)| (i, (), r.0.weight()))
            .collect();
        let k = self.pick_weighted(&cands).map(|k| cands[k].0).unwrap_or(0);
        self.remember(1000 + k);
        fill(REMARKS[k].2, &[], ctx.days)
    }

    /// Two people passing: (A's greeting, B's answer).
    pub fn greet(&mut self, ctx: &TalkCtx, x: f32, a: &str, b: &str) -> (String, String) {
        let cands: Vec<(usize, (), u32)> = GREETS.iter().enumerate().filter(|(_, g)| g.0.ok(ctx, x)).map(|(i, g)| (i, (), g.0.weight())).collect();
        let k = self.pick_weighted(&cands).map(|k| cands[k].0).unwrap_or(GREETS.len() - 1);
        (fill(GREETS[k].1, &[a, b], ctx.days), fill(GREETS[k].2, &[a, b], ctx.days))
    }

    /// Pick a topic for a chat with the player (index into the chats).
    pub fn chat_topic(&mut self, ctx: &TalkCtx, x: f32, kid: bool) -> usize {
        let cands: Vec<(usize, (), u32)> = CHATS
            .iter()
            .enumerate()
            .filter(|(i, ch)| ch.1 == kid && ch.0.ok(ctx, x) && !self.recent.contains(&(2000 + i)))
            .map(|(i, ch)| (i, (), ch.0.weight()))
            .collect();
        let k = self.pick_weighted(&cands).map(|k| cands[k].0).unwrap_or_else(|| CHATS.iter().position(|c| c.1 == kid).unwrap_or(0));
        self.remember(2000 + k);
        k
    }

    /// Step `step` of chat `topic`: (the player's line, their answer), or
    /// None once it's run its course.
    pub fn chat_line(&self, topic: usize, step: usize, days: i64) -> Option<(String, String)> {
        let (_, _, lines) = CHATS.get(topic)?;
        lines.get(step).map(|(a, b)| (fill(a, &[], days), fill(b, &[], days)))
    }

    /// Something to open with when meeting the player for the first time.
    pub fn intro(&mut self, ctx: &TalkCtx, x: f32, name: &str, kid: bool) -> String {
        let hello = if kid { format!("hi!! I'm {name}!") } else { format!("hello! I'm {name}.") };
        let tail = self.remark(ctx, x, kid);
        format!("{hello} {tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_fit_the_moment_and_use_names() {
        // (fresh each time: a single Talk deliberately avoids repeating itself)
        let mut ctx = TalkCtx { train: true, ..Default::default() };
        let mut topical = 0;
        for seed in 0..40 {
            let mut talk = Talk::new(seed);
            let c = talk.conversation(&ctx, 0.0, &["Martha", "Henry"], &[false, false]);
            assert!(c.len() >= 3, "{c:?}");
            assert!(c.iter().all(|(s, _)| *s < 2));
            if c.iter().any(|(_, l)| l.contains("train") || l.contains("station") || l.contains("driver")) {
                topical += 1;
            }
            assert!(!c.iter().any(|(_, l)| l.contains('{')), "unfilled: {c:?}");
        }
        assert!(topical > 10, "train talk only {topical}/40 times");
        // kids talk like kids
        let mut talk = Talk::new(99);
        ctx.train = false;
        let c = talk.conversation(&ctx, 0.0, &["Pip", "Dot"], &[true, true]);
        assert!(!c.is_empty());
        // a three-way chat uses all three
        let c = talk.conversation(&ctx, 0.0, &["Martha", "Henry", "Clara"], &[false; 3]);
        assert!(c.iter().any(|(s, _)| *s == 2));
    }

    #[test]
    fn chats_with_the_player_run_their_course() {
        let mut talk = Talk::new(2);
        let ctx = TalkCtx::default();
        let topic = talk.chat_topic(&ctx, 0.0, false);
        let mut n = 0;
        while talk.chat_line(topic, n, 5).is_some() {
            n += 1;
        }
        assert!((2..=4).contains(&n));
    }
}
