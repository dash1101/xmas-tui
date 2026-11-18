//! Talking with the villagers, story-game style: a dialogue box with what
//! they said and a few things to say back (1-5, or click one). Every
//! grown-up has a job and a mood, every kid a wish and something they're
//! up to (the same each time for the same person), and the talk can turn
//! to whatever's going on in the village. Some choices do things: ask
//! someone to walk along, hand over a cocoa, start a snowball fight.
//!
//! What's offered after an answer: first whatever follows on from it (a
//! question about what was just said), then the questions not asked yet,
//! then goodbye. Every choice gets a real answer, nothing's offered twice,
//! and nobody answers a question with a question.

use super::dialogue::{Talk, TalkCtx};
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::{hash, Rng};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Node {
    Hello,
    /// Back to the questions (what they said came with the choice).
    Hub,
    How,
    /// "anything I can do to help?": they'd love a cocoa; they're fine.
    HelpCocoa,
    HelpCheer,
    Job,
    JobMore,
    JobYears,
    /// A tip (which one).
    Tip(usize),
    Topic(usize, usize),
    Play,
    Wish,
    WishGood,
    WishHope,
    Santa,
    SantaLeave,
    SantaStay,
    /// Talking about something they noticed or are doing: (which, line).
    Aware(usize, usize),
    // ---- at home (talking with a host)
    HomeLovely,
    Family,
    Years,
    Cooking,
    PetTalk,
    HelpAsk,
    // ---- in the cocoa cabin
    CabinGood,
    CabinBusy,
    CabinCocoa,
    CabinOften,
    Bye,
}

/// What a host at home talks about (from the household).
#[derive(Clone, Debug)]
pub struct HostCtx {
    pub theme: super::interior::Theme,
    /// The others who live there (not visitors).
    pub family: Vec<&'static str>,
    pub pet: Option<(super::pets::PetKind, String)>,
    pub years: u32,
    pub church: bool,
    pub piano: bool,
    pub oven: bool,
    /// The job they'd ask for help with (picked when asked).
    pub task: Option<super::interior::Task>,
    /// In the cocoa cabin; the one behind its counter.
    pub cabin: bool,
    pub keeper: bool,
}

/// Something going on that a villager is aware of, to talk about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aware {
    /// A kid in the middle of building a snowman (or an igloo).
    Building { igloo: bool },
    /// A kid who helped build the snowman (igloo) standing nearby.
    Built { igloo: bool },
    /// Someone else's snowman (igloo) nearby.
    SeeBuild { igloo: bool },
    /// A kid in the middle of a snowball fight.
    Fighting,
    /// Saw the villager feed / pet an animal ("bunny", "deer", "fox").
    SawFeed(&'static str),
    SawPet(&'static str),
    /// Saw the villager throwing snowballs; was hit by one.
    SawSnowball,
    GotHit,
    /// Saw the villager give someone a present; was given one.
    SawGift,
    GotGift,
    /// Was carried about by the villager; saw someone else carried.
    WasCarried,
    SawCarry,
    /// Saw the villager come down out of the sky.
    SawDropIn,
    /// Saw the villager come out of a house.
    SawVisit,
    /// Talked with Santa from the ground just now (this villager did).
    TalkedToSanta,
    /// Out with their pet (kind, its name).
    HasPet(super::pets::PetKind, &'static str),
}

impl Aware {
    /// What they open with (if it's on their mind), and the things that can
    /// be said about it: (what the villager says, the reply, what it does).
    /// `opened`: they did open with it (the first line answers the opener
    /// then; otherwise it brings the thing up).
    fn lines(self, kid: bool, pick: usize, opened: bool) -> (Option<String>, Vec<(String, String, Option<Act>)>) {
        let l = |a: &str, b: &str| (a.to_string(), b.to_string(), None);
        // the first line: an answer to the opener, or out of the blue
        let first = |answer: (&str, &str), unasked: (&str, &str)| {
            let (a, b) = if opened { answer } else { unasked };
            (a.to_string(), b.to_string(), None)
        };
        let act = |a: &str, b: &str, x: Act| (a.to_string(), b.to_string(), Some(x));
        let names = ["Sir Snowsalot", "Frosty Junior", "Mister Carrots", "Big Steve", "Captain Chilly"];
        let name = names[pick % names.len()];
        match self {
            Aware::Building { igloo: false } => (
                Some("we're building a snowman! want to help?".into()),
                vec![
                    act(if opened { "I'd love to help!" } else { "need a hand with the snowman?" }, "YES! you pack the snow, I'll roll it!", Act::HelpBuild),
                    l("what are you going to call him?", &format!("{name}. it's the best name ever")),
                    l("it's going to be huge!", "the BIGGEST one ever. bigger than the big tree"),
                ],
            ),
            Aware::Building { igloo: true } => (
                Some("we're making an igloo! it's going to be HUGE".into()),
                vec![
                    act("can I help build it?", "grab some blocks! pack them tight!", Act::HelpBuild),
                    l("how's it coming along?", "slowly. the blocks keep falling over"),
                    l("will I fit inside?", "hmm… if you squeeze. maybe. no."),
                ],
            ),
            Aware::Built { igloo: false } => (
                Some("did you see our snowman? we MADE it!".into()),
                vec![first(("I saw! it's brilliant", "thanks! the hat was MY idea"), ("did you make that snowman?", "we did! the hat was MY idea")), l("does he have a name?", &format!("{name}! say hi to him!"))],
            ),
            Aware::Built { igloo: true } => (
                Some("we built an IGLOO! did you see it?".into()),
                vec![first(("I did! is it cozy inside?", "SO cozy. it's members only, though"), ("did you build that igloo?", "we did! it's SO cozy inside. members only, though")), l("can I join the igloo club?", "you have to know the secret knock. and I'm not telling")],
            ),
            Aware::SeeBuild { igloo } => {
                let thing = if igloo { "igloo" } else { "snowman" };
                if kid {
                    (None, vec![l(&format!("did you build that {thing}?"), "no, but I helped! a bit. I watched")])
                } else {
                    (None, vec![l(&format!("who built that {thing}?"), "the kids. took them ages, and they're very proud of it")])
                }
            }
            Aware::Fighting => (
                Some("can't talk, SNOWBALL FIGHT!".into()),
                vec![l("who's winning?", "US! obviously!"), act("can I join in?", "you're on OUR team! …just kidding. INCOMING!", Act::Snowball)],
            ),
            Aware::SawFeed(a) => {
                if kid {
                    (Some(format!("you fed the {a}! that was SO cool")), vec![first(("want to try next time?", "YES! I'll be really, really gentle"), (&format!("the {a} ate right out of my hand!"), "I SAW! can I try next time?"))])
                } else {
                    (Some(format!("I saw you feed that {a}. it's taken a shine to you")), vec![first(("it ate right out of my hand!", "you've a way with animals, you have"), (&format!("that {a} ate right out of my hand!"), "I saw! you've a way with animals, you have"))])
                }
            }
            Aware::SawPet(a) => (
                None,
                vec![if kid { l(&format!("a {a} just let me pet it!"), "it LET you? they never let me") } else { l(&format!("that {a} let me pet it!"), "they're tame as anything round here, if you're gentle") }],
            ),
            Aware::SawSnowball => (
                None,
                vec![if kid { l("nice throw, huh?", "it was okay. I could do better") } else { l("sorry if that snowball came close", "ha! I've thrown a few in my time") }],
            ),
            Aware::GotHit => {
                if kid {
                    (Some("hey! you got me with that snowball!".into()), vec![first(("sorry! I couldn't resist", "the next one's coming YOUR way"), ("sorry about that snowball!", "the next one's coming YOUR way")), act("rematch?", "oh, it is SO on", Act::Snowball)])
                } else {
                    (Some("you got me with a snowball, you rascal!".into()), vec![first(("sorry! I couldn't resist", "ha! I'd have done the same at your age"), ("sorry about that snowball!", "ha! I'd have done the same at your age"))])
                }
            }
            Aware::SawGift => {
                if kid {
                    (Some("you gave someone a PRESENT! I saw!".into()), vec![first(("maybe there's one for you, if you're good…", "I'm being SO good right now. look"), ("I've been handing out presents", "is there one for me? I've been SO good"))])
                } else {
                    (Some("I saw you give someone a present. that was kind of you".into()), vec![first(("it's Christmas, after all", "it is. and you've got the spirit of it"), ("I've been handing out presents", "I saw. that was kind of you"))])
                }
            }
            Aware::GotGift => (
                Some(if kid { "thank you for the present! I'm saving it".into() } else { "thank you again for the present. so thoughtful".into() }),
                vec![first(("I hope you like it!", if kid { "I LOVE it. I'm not sharing it" } else { "I do. I'll treasure it" }), ("did you like the present?", if kid { "I LOVE it. I'm not sharing it" } else { "very much. I'll treasure it" }))],
            ),
            Aware::WasCarried => {
                if kid {
                    (Some("do the flying thing again!".into()), vec![first(("hold on tight next time!", "I WILL. again! again!"), ("sorry for carrying you off like that!", "don't be sorry! do it again!"))])
                } else {
                    (Some("you carried me halfway across the village!".into()), vec![first(("sorry about that!", "my feet never touched the ground. I'm still dizzy"), ("sorry for carrying you off like that!", "my feet never touched the ground. I'm still dizzy"))])
                }
            }
            Aware::SawCarry => (None, vec![l("I gave someone a lift just now", if kid { "can you carry ME next?" } else { "I saw! they looked a bit dizzy afterwards" })]),
            Aware::SawDropIn => (
                Some(if kid { "did you just fall out of the SKY?".into() } else { "did you just… come down out of the sky?".into() }),
                vec![first(("I did! it's a long way down", if kid { "teach me! teach me!" } else { "I nearly dropped my cocoa" }), ("I just came down out of the sky, you know", if kid { "I SAW! teach me! teach me!" } else { "I saw! I nearly dropped my cocoa" }))],
            ),
            Aware::SawVisit => (None, vec![l("I've just been visiting next door", if kid { "lucky! they always have cookies in there" } else { "lovely, aren't they? the kettle's always on in there" })]),
            Aware::HasPet(k, n) => {
                use super::pets::PetKind as P;
                let facts: [&str; 3] = match k {
                    P::Dog => ["follows me everywhere. and I mean everywhere", "never met a puddle without jumping in it", "three years old, and still thinks snow is brand new"],
                    P::Cat => ["only came out because I had treats", "walks me, really. not the other way round", "is judging your boots. affectionately"],
                    P::Bunny => ["does a little hop when happy. watch", "braver than you'd think. chased a fox once", "had four carrots today. it's a holiday"],
                    P::Reindeer => ["in training for the sleigh. don't laugh", "thinks every bell is for them", "had six carrots today. it's a holiday"],
                    P::Fox => ["found me, really. not the other way round", "buries my gloves in the snow", "sweeter than you'd think"],
                    P::Bird => ["sings at my window every morning", "rides about on my hat", "knows three carols. well, the first lines"],
                };
                let fact = facts[pick % 3];
                (None, vec![l("and who's this?", &format!("this is {n}! {fact}")), l(&format!("can I pet {n}?"), &format!("of course! {n} loves the attention"))])
            }
            Aware::TalkedToSanta => (
                Some(if kid { "I talked to SANTA! did you see?".into() } else { "I just spoke to Santa! can you believe it?".into() }),
                vec![first(("I saw! what did he say?", if kid { "he said I'm on the nice list! I think" } else { "he remembered my name. after all these years" }), ("was that you talking to Santa?", if kid { "it was ME! he said I'm on the nice list! I think" } else { "it was! he remembered my name. after all these years" }))],
            ),
        }
    }
}

/// What a choice leads to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    Go(Node),
    /// Ask them to walk along.
    Invite,
    /// A kid's up for a snowball fight with the player.
    Snowball,
    /// Hand them a cocoa from the pockets.
    GiveCocoa,
    /// Lend a hand with what they're building.
    HelpBuild,
    // ---- at home
    /// Take a seat (they offered).
    Sit,
    /// A cookie for the pockets.
    Cookie,
    /// Take on a little job for them.
    Task(super::interior::Task),
    /// They play a tune on the piano.
    PlayTune,
    /// Something to do with the villager's own pet.
    Pet(PetAct),
}

/// The villager's pet menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PetAct {
    Fuss,
    /// Dinner from a bowl; a cuddle in the villager's arms; nap time (and
    /// waking up); a good brush; the page with the rest.
    Feed,
    Cuddle,
    Nap,
    Wake,
    Brush,
    More,
    /// The play menu; throw a toy (pets::ToyKind as 0 yarn, 1 ball, 2
    /// stick, 3 carrot); a trick; "chase me!"; a dangled scarf.
    Play,
    Throw(u8),
    Trick,
    Zoomies,
    Tease,
    Come,
    Stay,
    /// A page of names to choose from; a name (index into the names).
    Names(u8),
    Name(u8),
    /// Their look: a different kind (0 dog, 1 cat, 2 bunny), the next colour.
    Look,
    Kind(u8),
    Coat,
    Done,
}

impl Opt {
    pub fn new(label: &str, act: Act) -> Opt {
        Opt { label: label.to_string(), act, reply: None, tag: None }
    }
}

pub struct Opt {
    pub label: String,
    pub act: Act,
    /// What they say back (for answers that carry their own).
    reply: Option<String>,
    /// Marks the choice as taken (it isn't offered again).
    tag: Option<u8>,
}

pub struct Convo {
    pub id: u64,
    pub name: String,
    pub kid: bool,
    /// What they just said, and what can be said back.
    pub line: String,
    pub opts: Vec<Opt>,
    /// Exchanges so far; when it ended (the box lingers a moment).
    pub steps: u32,
    pub ended_at: Option<f32>,
    /// Choices already taken (not offered again).
    asked: Vec<u8>,
    /// Who they are (stable per person).
    seed: u32,
    topic: Option<usize>,
    /// What they're aware of (most on their mind first), and which of it
    /// they opened with.
    aware: Vec<Aware>,
    opened: Option<usize>,
    /// Where the box was dragged to (top-left), if it was.
    pub pos: Option<(i32, i32)>,
    /// How they talk (the same each time for the same person).
    pub temper: Temper,
    /// Talking with someone in their own home.
    pub host: Option<HostCtx>,
    /// Times talked with before this.
    visits: u32,
    /// Already walking along with the villager (not asked to again).
    along: bool,
    /// Indoors as someone's guest (no snowball fights).
    indoors: bool,
    rng: Rng,
}

/// What someone does, and what can be asked about it.
struct Job {
    /// What they say they do (to "what do you do around here?").
    says: &'static str,
    /// A question about that, and its answer.
    ask: &'static str,
    more: &'static str,
    /// "how long…?", and its answer.
    long: &'static str,
    years: &'static str,
}

const JOBS: &[Job] = &[
    Job {
        says: "I'm the baker. I'm up at four every morning for the gingerbread",
        ask: "four in the morning?!",
        more: "you get used to it. and by six the whole street smells of ginger. worth it",
        long: "how long have you been baking?",
        years: "since I could reach the counter. my mother ran the bakery before me",
    },
    Job {
        says: "I'm the stationmaster. I keep the trains running on time. mostly",
        ask: "mostly?",
        more: "the 6:15 has never once been late. the 6:40… we don't talk about the 6:40",
        long: "how long have you worked at the station?",
        years: "twenty-two years. I can tell every train by its whistle",
    },
    Job {
        says: "I teach at the school. the kids are a handful this close to Christmas",
        ask: "what are they up to?",
        more: "we're putting on a nativity play, and three of them want to be the donkey",
        long: "how long have you been teaching?",
        years: "eleven years. some of my first class have kids of their own now",
    },
    Job {
        says: "I'm a carpenter. I made half the sleds in this village",
        ask: "what makes a good sled?",
        more: "wax on the runners. lots of wax. then it goes like the wind",
        long: "how long have you been making them?",
        years: "thirty winters. my very first one's still going. my nephew has it",
    },
    Job {
        says: "I make candles. every window light you see? probably one of mine",
        ask: "which ones sell best?",
        more: "cinnamon, every year. pine for the traditional folk",
        long: "how long have you been making candles?",
        years: "since I was little. my gran taught me at her kitchen table",
    },
    Job {
        says: "I run the post office. it's letters to Santa all month. sacks of them",
        ask: "do you ever read them?",
        more: "never. …well, only the ones with drawings on the envelope",
        long: "how long have you run the post office?",
        years: "fifteen years. never lost a single letter. misplaced a few",
    },
    Job {
        says: "I lead the choir. we sing at the church on Christmas Eve",
        ask: "is it hard work?",
        more: "keeping the tenors in tune is. we could use another, if you sing",
        long: "how long have you led the choir?",
        years: "nine years. before that I was the loudest tenor in it",
    },
    Job {
        says: "I fish through the ice out on the lake, most mornings",
        ask: "caught anything good?",
        more: "a trout THIS big, once. well, this big. it gets smaller every time I tell it",
        long: "how long have you been fishing?",
        years: "since I was a kid. it's the quiet I go for, really",
    },
    Job {
        says: "I make toys. wooden trains, mostly, and the odd rocking horse",
        ask: "do you make any for Santa?",
        more: "between you and me, he buys a few every year. wholesale",
        long: "how long have you been making toys?",
        years: "twenty-odd years. I still test every one myself. very thoroughly",
    },
    Job {
        says: "I'm retired. forty years at sea, and now I just watch the snow",
        ask: "do you miss the sea?",
        more: "some days. but nothing's as quiet as a snowy night. not even the ocean",
        long: "how long have you lived here?",
        years: "twelve winters now. I came for one and never left",
    },
    Job {
        says: "I'm the librarian. it's warm, it's quiet, and it's full of stories",
        ask: "got a good book for this time of year?",
        more: "a ghost story by the fire. trust me. come by and I'll find you one",
        long: "how long have you worked at the library?",
        years: "eighteen years. I've read about half of it",
    },
    Job {
        says: "I keep cows on the farm up the hill. they don't care that it's Christmas",
        ask: "do they get anything special?",
        more: "extra hay on Christmas morning. it's tradition",
        long: "how long have you had the farm?",
        years: "it's been in the family a hundred years. I've done thirty of them",
    },
];

/// Things worth knowing about the village (all true of the game, and said
/// the same way on any device).
const TIPS: &[&str] = &[
    "shake a bush and berries fall out. the deer will follow you anywhere for one",
    "knock on any door with a light on. most folk will ask you in",
    "the cocoa cabin sells lanterns. handy once it gets dark",
    "keep an eye on the moon. poke it, even. something happens, but I won't spoil it",
    "if Santa's sleigh goes over, wave him down. he'll sometimes stop for a word",
    "whatever you do, don't go poking the power lines up by the hills",
    "the kids will never say no to a snowball fight. just ask one",
    "go up high enough and the stars make pictures. my grandad swore by it",
];

/// What a kid's up to, and what they say to "can I help?".
const PLAY: &[(&str, &str)] = &[
    ("I'm looking for reindeer tracks. I found some! …I think they're a dog's", "okay! you look by the trees, I'll look by the fence"),
    ("I'm counting the lights on the big tree. I'm up to three hundred!", "okay, you count the top half. I keep losing my place"),
    ("I'm practising my snowball throw. I can nearly hit the lamp post", "you can stand by the lamp post! …just kidding"),
    ("I'm catching snowflakes on my tongue. I've had about a hundred", "okay! but the big ones are mine"),
    ("I'm making snow angels. I've done eleven!", "make one next to mine, then they can be friends"),
    ("I'm looking for the longest icicle in the whole village", "okay! but if you find a good one, it's mine"),
];
/// What a kid wants for Christmas, and (same kid) what they got.
const WISHES: &[&str] = &[
    "a sled. a red one. a FAST one",
    "a puppy. a real one, not a toy one",
    "a telescope, so I can watch for Santa",
    "a drum kit! mum says no, but Santa might say yes",
    "a dinosaur. a small one",
    "snow that never, ever melts",
];
const GOT: &[&str] = &[
    "a sled! a red one! it's SO fast",
    "a puppy! well, a puppy you cuddle, not a real one. it's still a puppy",
    "a telescope! I'm going to look at the moon all night",
    "a drum kit! mum's already sorry",
    "a book about dinosaurs. it's almost as good as a dinosaur",
    "a snow globe. so it's snowing in there forever. that counts",
];
const GOOD: &[&str] = &[
    "SO good. mostly. okay, a little bit good",
    "I tidied my room! well, I pushed everything under the bed",
    "I shared my cookies. one of them. a small one",
    "I've been the goodest. ask anyone. not my brother, though",
];

/// Christmas has been (it's the day, or the days after).
fn xmas_over(ctx: &TalkCtx) -> bool {
    ctx.days == 0 || ctx.days > 330
}

/// How someone talks. Everyone has one (the same each time), and it colours
/// their hellos, how they're doing and how they say goodbye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Temper {
    /// Motherly: "dear", wrap up warm, have a biscuit.
    Warm,
    /// Can't stop talking (in the nicest way).
    Chatty,
    /// Quiet, blushes, glad of the company though.
    Shy,
    /// A twinkle in the eye; teases.
    Cheeky,
    /// Head in the clouds, notices the small beautiful things.
    Dreamy,
    /// Gruff on the outside, soft as snow on the inside.
    Gruff,
}

const TEMPERS: [Temper; 6] = [Temper::Warm, Temper::Chatty, Temper::Shy, Temper::Cheeky, Temper::Dreamy, Temper::Gruff];
const KID_TEMPERS: [Temper; 4] = [Temper::Chatty, Temper::Shy, Temper::Cheeky, Temper::Dreamy];

/// Lines per temperament: (first hello with {name}, hello again, hello
/// again to a regular, how they are (grown-ups), goodbye). None of them
/// says what time of day it is (they're said at any hour), and none asks
/// the villager something there'd be no way to answer.
struct Voice {
    hi: &'static [&'static str],
    again: &'static [&'static str],
    regular: &'static [&'static str],
    mood: &'static [&'static str],
    bye: &'static [&'static str],
}

fn voice(t: Temper, kid: bool) -> Voice {
    match (t, kid) {
        (Temper::Warm, _) => Voice {
            hi: &["oh, hello, dear! I'm {name}. I don't think we've met", "well, hello there! I'm {name}. you look frozen. come and stand by the lamp", "hello, dear! I'm {name}. merry Christmas!"],
            again: &["there you are! I was hoping you'd stop by", "hello again, dear! you've got snow all over your shoulders", "oh, it's you! I hope that scarf's warm enough"],
            regular: &["my favourite neighbour! it's always lovely to see you", "back again! you're always welcome, you know"],
            mood: &["all the better for seeing you, dear", "warm as toast. I've had two cocoas and a mince pie", "a bit tired, but the good kind of tired"],
            bye: &["off you go, and wrap up warm, dear!", "come by for cocoa any time. I mean it!", "mind the ice, now. merry Christmas!"],
        },
        (Temper::Chatty, false) => Voice {
            hi: &["hello! I'm {name}! I don't think we've met. isn't it all so lovely?", "oh, hello! {name}, that's me. have you tried the cocoa yet? you really must", "{name}! pleased to meet you! I do love meeting new people"],
            again: &["you again! oh good, I was getting bored of my own company", "hello, hello! I was just thinking about you", "oh, perfect timing! I was about to go and get a cocoa"],
            regular: &["my favourite listener! I could talk to you all day", "you're becoming a regular! I love that"],
            mood: &["wonderful! I've had three cocoas already and I'm not slowing down", "busy, busy! I've wrapped forty presents. well, thirty. ish", "fantastic! my aunt's coming tomorrow and she always brings fudge"],
            bye: &["oh, I could talk all day! bye for now!", "wait, one more thing— oh, never mind. merry Christmas!", "bye! come and find me again soon!"],
        },
        (Temper::Shy, false) => Voice {
            hi: &["oh! um… hi. I'm {name}.", "h-hello. I'm {name}. merry Christmas…", "oh. hi. I'm {name}."],
            again: &["oh, hi again", "you came back to say hi? that's… really nice", "oh. hello again. I hoped you would"],
            regular: &["hi! …I mean, hello. it's always nice when you stop by", "hi again. I was hoping I'd see you"],
            mood: &["good, thanks… a little cold", "okay! I found a really nice icicle earlier", "good. better now, actually"],
            bye: &["bye… thanks for talking to me", "merry Christmas. …see you around?", "bye! um. see you"],
        },
        (Temper::Cheeky, false) => Voice {
            hi: &["well, well, a face I don't know! {name}, at your service", "{name}. charming, modest, and excellent at snowball fights", "I'm {name}. you'll want to stay on my good side"],
            again: &["back for more of my wisdom?", "ha! couldn't stay away, could you?", "you again! people will start to talk"],
            regular: &["my number one fan returns!", "at this rate I'll have to put you in my will"],
            mood: &["freezing, starving, and loving every minute", "never better. I'm on the nice list. probably", "excellent. I've just hidden a snowball in the postbox. don't tell"],
            bye: &["don't do anything I wouldn't do!", "off you go. if anyone asks, you never saw me", "see you around! merry Christmas!"],
        },
        (Temper::Dreamy, false) => Voice {
            hi: &["hello… I'm {name}. isn't the snow beautiful?", "oh, hello. I'm {name}. I was just counting snowflakes", "I'm {name}. listen… you can almost hear the lights hum"],
            again: &["oh, it's you… hello again", "hello again. I was hoping you'd wander by", "you're back. I was just thinking of you. and of cinnamon"],
            regular: &["hello, friend. the village feels warmer when you're about", "oh, good. you always turn up at the nicest moments"],
            mood: &["I've been watching the snowflakes. no two the same, you know", "peaceful. like the hush after the carols stop", "a little far away, but the nice kind of far"],
            bye: &["goodbye for now. listen out for sleigh bells", "take some of the quiet with you", "go gently. and look up now and then"],
        },
        (Temper::Gruff, false) => Voice {
            hi: &["{name}. …don't think we've met. mind the ice", "hmph. {name}. …merry Christmas, I suppose", "{name}. don't step on the flowerbed. there's one under there somewhere"],
            again: &["you again. …good. it's nice to see you", "hmph. back, are you? well. hello", "oh, it's you. …almost glad to see you"],
            regular: &["you're alright, you know that? don't tell anyone I said so", "you again. good. the village needs folk like you"],
            mood: &["cold. but… not bad. not bad at all", "my knees know it's December. otherwise fine", "fine. the cocoa helps. don't tell the cabin I said so"],
            bye: &["go on, then. …merry Christmas", "off with you. and wear a hat", "hmph. take care of yourself"],
        },
        // kids (they're asked what they're up to, not how they are)
        (Temper::Chatty, true) => Voice {
            hi: &["hi! I'm {name}! do you like snow? I LOVE snow", "hiya! I'm {name}! I'm six! and three quarters!", "hi! I'm {name}! I made a snow angel. it's over there somewhere"],
            again: &["it's YOU! hi hi hi!", "you came back! I knew you would", "hiii! I was hoping you'd come!"],
            regular: &["you're my best friend now. that's just how it is", "HI! I told everyone about you!"],
            mood: &[],
            bye: &["bye! come back soon!", "BYE! merry Christmas! bye!", "see ya! don't forget me!"],
        },
        (Temper::Shy, true) => Voice {
            hi: &["…hi. I'm {name}.", "oh! um. I'm {name}.", "hi… I'm {name}. I like your hat"],
            again: &["oh! it's you… hi", "hi again", "you came back… cool"],
            regular: &["hi! …I saved you a good snowball", "hi! I was hoping it was you"],
            mood: &[],
            bye: &["bye…", "bye! …thanks for talking to me", "merry Christmas… bye"],
        },
        (Temper::Cheeky, true) => Voice {
            hi: &["I'm {name}! I'm the best snowball thrower in the WORLD", "hi! I'm {name}. bet you can't catch me", "I'm {name}! I know a secret. …I'm not telling"],
            again: &["you again! ready to lose a snowball fight?", "ha! you're back! tag, you're it!", "hi! I've still got a snowball with your name on it"],
            regular: &["you're back! I've been practising my throw", "my favourite person! don't tell my mum"],
            mood: &[],
            bye: &["bye! watch your back! hehe", "see ya, slowpoke!", "bye! merry Christmas! …duck!"],
        },
        (_, true) => Voice {
            hi: &["hi… I'm {name}. I think the snowflakes can hear us", "I'm {name}. I'm watching the clouds", "hello. I'm {name}. the moon follows me everywhere"],
            again: &["oh, it's you! hello", "hi again! look, the snow's all sparkly", "you're back! I was just thinking about reindeer"],
            regular: &["hi, friend. I drew a picture of you. in the snow", "hello! you always come at the best bits"],
            mood: &[],
            bye: &["bye… look out for reindeer", "bye! look up at the stars later", "merry Christmas… bye"],
        },
    }
}

/// Someone's temperament (the same every time for the same person).
pub fn temper_of(id: u64, kid: bool) -> Temper {
    let h = hash(id as u32 ^ 0x7e3b) as usize;
    if kid {
        KID_TEMPERS[h % KID_TEMPERS.len()]
    } else {
        TEMPERS[h % TEMPERS.len()]
    }
}

/// A friend's (or family member's) first words: they know the villager.
pub fn friend_hello(you: &str, me: &str, kid: bool, roll: u32) -> String {
    let lines: &[&str] = match (kid, you.is_empty()) {
        (false, false) => &["{you}! so good to see you", "{you}! I was hoping I'd bump into you", "there you are, {you}! isn't the village lovely?", "{you}! it's me, {me}. fancy meeting you here"],
        (true, false) => &["{you}! it's me, {me}!", "{you}! come and play!", "{you}! I came to see the big tree!", "{you}! guess what? it's nearly Christmas!"],
        (false, true) => &["there you are! so good to see you", "oh, it's you! hello, old friend", "it's me, {me}! I was hoping I'd see you"],
        (true, true) => &["it's me, {me}! come and play!", "hi! I came to see the big tree!", "it's you! hi!"],
    };
    lines[roll as usize % lines.len()].replace("{you}", you).replace("{me}", me)
}

/// A friend calling out as they come by.
pub fn friend_call(you: &str, kid: bool, roll: u32) -> String {
    let lines: &[&str] = match (kid, you.is_empty()) {
        (false, false) => &["{you}! hello!", "{you}! over here!", "oh, {you}! merry Christmas!"],
        (true, false) => &["{you}! hi!", "{you}! {you}! over here!", "hi {you}!"],
        (false, true) => &["hello, you! over here!", "oh, it's you! merry Christmas!"],
        (true, true) => &["hi! over here!", "it's you! hi!"],
    };
    lines[roll as usize % lines.len()].replace("{you}", you)
}

/// A hello with the villager's name in it (when they've picked one): on a
/// first meeting, now and then word has got round; after that, people
/// greet them by name. (For a first meeting where the villager has just
/// said who they are, see `met`.)
pub fn by_name(line: &str, you: &str, first: bool, kid: bool, roll: u32) -> String {
    if you.is_empty() {
        return line.to_string();
    }
    match (first, kid, roll % 3) {
        (true, false, 0) => format!("{line} you must be {you}. word gets round in a village this size"),
        (true, true, 0) => format!("{line} wait, are you {you}? everyone's talking about you!"),
        (true, ..) => line.to_string(),
        (false, true, 0 | 1) => format!("{you}! {line}"),
        (false, false, 0) => format!("{you}! {line}"),
        (false, false, 1) => format!("ah, {you}. {line}"),
        _ => line.to_string(),
    }
}

/// Two things said one after the other (a full stop between, unless the
/// first already ends a sentence).
fn join(a: &str, b: &str) -> String {
    if a.ends_with(['!', '?', '.', '…']) {
        format!("{a} {b}")
    } else {
        format!("{a}. {b}")
    }
}

/// A first hello, just after the villager said "hello! I'm {you}".
pub fn met(line: &str, you: &str, kid: bool, roll: u32) -> String {
    if you.is_empty() {
        return line.to_string();
    }
    match (kid, roll % 3) {
        (false, 0) => format!("{line} lovely to meet you, {you}"),
        (false, 1) => format!("{line} nice to meet you, {you}."),
        (true, 0) => format!("{line} I've heard about you, {you}!"),
        _ => line.to_string(),
    }
}

impl Convo {
    /// Start a conversation with `name` (`visits`: how many times the
    /// villager has talked with them before; 0 = never met).
    #[allow(clippy::too_many_arguments)]
    pub fn open(id: u64, name: &str, kid: bool, visits: u32, ctx: &TalkCtx, talk: &mut Talk, x: f32, seed: u32, aware: Vec<Aware>) -> Self {
        let first = visits == 0;
        let mut c = Convo {
            id,
            name: name.to_string(),
            kid,
            line: String::new(),
            opts: vec![],
            steps: 0,
            ended_at: None,
            asked: vec![],
            seed: hash(id as u32 ^ 0x5707),
            topic: None,
            aware,
            opened: None,
            pos: None,
            temper: Temper::Warm,
            host: None,
            visits,
            along: false,
            indoors: false,
            rng: Rng::new((seed ^ id as u32) as u64 ^ 0x5a1),
        };
        c.temper = temper_of(id, kid);
        // something topical to ask about, if there's anything going on
        let topic = talk.chat_topic(ctx, x, kid);
        c.topic = talk.chat_line(topic, 0, ctx.days).map(|_| topic);
        // whatever's most on their mind comes first
        let pick = c.pick_for(7, 97);
        let opener = c.aware.iter().enumerate().find_map(|(i, a)| a.lines(kid, pick, true).0.map(|o| (i, o)));
        c.opened = opener.as_ref().map(|o| o.0);
        let opener = opener.map(|o| o.1);
        let v = voice(c.temper, kid);
        let hello = if first {
            c.rng.pick(v.hi).replace("{name}", name)
        } else if visits >= 3 {
            c.rng.pick(v.regular).to_string()
        } else {
            c.rng.pick(v.again).to_string()
        };
        c.line = match opener {
            Some(o) if first => join(&hello, &o),
            Some(o) => o,
            // now and then a word about what's going on, to them (and not
            // what the topic on offer is about)
            None if !first && c.rng.chance(0.5) => match talk.aside(ctx, x, kid, c.topic) {
                Some(a) => join(&hello, &a),
                None => hello,
            },
            None => hello,
        };
        c.opts = c.options(Node::Hello, ctx, talk, false);
        c
    }

    /// A little menu in the dialogue box (the pet's): a title, what's
    /// said, and the choices.
    pub fn menu(id: u64, title: &str, line: &str, opts: Vec<Opt>) -> Self {
        Convo {
            id,
            name: title.to_string(),
            kid: false,
            line: line.to_string(),
            opts,
            steps: 0,
            ended_at: None,
            asked: vec![],
            seed: 0,
            topic: None,
            aware: vec![],
            opened: None,
            pos: None,
            temper: Temper::Warm,
            host: None,
            visits: 0,
            along: false,
            indoors: false,
            rng: Rng::new(id),
        }
    }

    /// A conversation with someone in their own home (index `i` in the
    /// household; `visits` as for `open`).
    #[allow(clippy::too_many_arguments)]
    pub fn host(id: u64, name: &str, kid: bool, temper: Temper, visits: u32, ctx: &TalkCtx, talk: &mut Talk, h: HostCtx, seed: u32) -> Self {
        let mut c = Convo::open(id, name, kid, visits, ctx, talk, 0.0, seed, vec![]);
        c.temper = temper;
        c.topic = None;
        c.indoors = true;
        let first = visits == 0;
        let welcome: &[&str] = if h.church {
            &["welcome, friend. sit a while, if you like", "peace be with you. it's warmer in here, isn't it?"]
        } else if h.cabin && h.keeper {
            &["welcome to the cabin! the cocoa's hot", "come in out of the cold! the cocoa's just made"]
        } else if h.cabin {
            &["oh, hello! the cocoa here's the best in town", "hello! come and warm up"]
        } else {
            match temper {
                Temper::Warm => &["come in, come in! let me take your coat, dear", "you're frozen! sit by the fire, I'll put the kettle on"],
                Temper::Chatty => &["oh, a visitor! come in! excuse the mess. well, it's always like this", "come in! you have to see the tree, we did it ourselves!"],
                Temper::Shy => &["oh, hello. please, come in… make yourself at home", "h-hi. um. welcome. would you like to sit?"],
                Temper::Cheeky => &["ah, you've found our hideout! wipe your boots, mind", "welcome, welcome! no snowballs indoors. that's the rule. mostly"],
                Temper::Dreamy => &["hello… come in out of the snow. listen to the fire", "oh, how lovely. a visitor on a snowy day"],
                Temper::Gruff => &["hmph. come in, then. shut the door, you're letting the warm out", "well, don't stand there. come in. sit"],
            }
        };
        let hello = if first { join(c.rng.pick(welcome), &format!("I'm {name}.")) } else { c.rng.pick(welcome).to_string() };
        c.line = match (kid && !h.cabin, first) {
            (true, true) => format!("hi! this is MY house! I'm {name}!"),
            (true, false) => "you came back! come in, come in!".to_string(),
            _ => hello,
        };
        c.host = Some(h);
        c.opts = c.options(Node::Hello, ctx, talk, false);
        c
    }

    /// They're already walking along with the villager: not asked to again.
    pub fn walking_along(&mut self) {
        self.along = true;
        self.opts.retain(|o| o.act != Act::Invite);
    }

    /// A visitor in someone's home with the villager: along already, and no
    /// snowball fights indoors.
    pub fn visiting(&mut self) {
        self.indoors = true;
        self.walking_along();
        self.opts.retain(|o| o.act != Act::Snowball);
    }

    fn pick_for(&self, salt: u32, n: usize) -> usize {
        (hash(self.seed ^ salt) as usize) % n
    }

    fn job(&self) -> &'static Job {
        &JOBS[self.pick_for(1, JOBS.len())]
    }

    /// The next tip not given yet in this conversation.
    fn next_tip(&self) -> Option<usize> {
        let start = self.pick_for(16, TIPS.len());
        (0..TIPS.len()).map(|k| (start + k) % TIPS.len()).find(|&k| !self.asked.contains(&(40 + k as u8)))
    }

    /// What can be said back after `node`: what follows on from it, then
    /// the questions not asked yet (four in all), then goodbye.
    fn options(&self, node: Node, ctx: &TalkCtx, talk: &Talk, cocoa: bool) -> Vec<Opt> {
        if node == Node::Bye {
            return vec![];
        }
        let mut v = self.follow_ups(node, ctx, talk, cocoa);
        v.retain(|o| o.tag.is_none_or(|t| !self.asked.contains(&t)));
        for o in self.hub(ctx, talk) {
            if v.len() >= 4 {
                break;
            }
            if !v.iter().any(|w| w.label == o.label) {
                v.push(o);
            }
        }
        v.push(Opt::new(if self.kid { "see you later!" } else { "goodbye!" }, Act::Go(Node::Bye)));
        v
    }

    /// Lines about what they've noticed, not said yet: the first about each
    /// thing (or, `only` one of them, all that's left about it).
    fn aware_opts(&self, only: Option<usize>) -> Vec<Opt> {
        let pick = self.pick_for(7, 97);
        let mut v = vec![];
        for (i, a) in self.aware.iter().enumerate().take(3) {
            if only.is_some_and(|k| k != i) {
                continue;
            }
            let (_, lines) = a.lines(self.kid, pick, self.opened == Some(i));
            for (j, (label, reply, act)) in lines.into_iter().enumerate() {
                let tag = (100 + i * 10 + j) as u8;
                if self.asked.contains(&tag) {
                    continue;
                }
                v.push(Opt { label, act: act.unwrap_or(Act::Go(Node::Aware(i, j))), reply: Some(reply), tag: Some(tag) });
                if only.is_none() {
                    break;
                }
            }
        }
        v
    }

    /// The questions that can be asked any time (those not asked yet).
    fn hub(&self, ctx: &TalkCtx, talk: &Talk) -> Vec<Opt> {
        let o = |tag: u8, label: &str, act: Act| Opt { label: label.to_string(), act, reply: None, tag: Some(tag) };
        let mut v = vec![];
        if let Some(h) = &self.host {
            if h.cabin {
                if h.keeper {
                    v.push(o(32, "what's good here?", Act::Go(Node::CabinGood)));
                    v.push(o(33, "busy today?", Act::Go(Node::CabinBusy)));
                    v.push(o(23, "how long have you run the cabin?", Act::Go(Node::Years)));
                    v.push(o(27, "mind if I sit down?", Act::Sit));
                } else {
                    v.push(o(34, "how's the cocoa?", Act::Go(Node::CabinCocoa)));
                    v.push(o(35, "do you come here often?", Act::Go(Node::CabinOften)));
                    v.push(o(27, "is this seat free?", Act::Sit));
                }
            } else {
                let lovely = if h.church {
                    "it's beautiful in here"
                } else if self.kid {
                    "I like your house!"
                } else {
                    "your home is lovely"
                };
                let years = if h.church { "how long have you looked after the church?" } else { "how long have you lived here?" };
                for (tag, label, act, ok) in [
                    (21, lovely, Act::Go(Node::HomeLovely), true),
                    (22, "tell me about your family", Act::Go(Node::Family), !h.family.is_empty() && !self.kid && !h.church),
                    (23, years, Act::Go(Node::Years), true),
                    (24, "something smells wonderful…", Act::Go(Node::Cooking), h.oven && !h.church),
                    (25, "can I help with anything?", Act::Go(Node::HelpAsk), !h.church),
                    (26, "and who's this?", Act::Go(Node::PetTalk), h.pet.is_some() && !h.church),
                    (27, "mind if I sit down?", Act::Sit, true),
                    (28, "would you play something on the piano?", Act::PlayTune, h.piano),
                ] {
                    if ok {
                        v.push(o(tag, label, act));
                    }
                }
            }
        } else {
            // things they've noticed come first
            v.extend(self.aware_opts(None).into_iter().take(2));
            let topic = self.topic.and_then(|t| talk.chat_line(t, 0, ctx.days).map(|(mine, _)| (t, mine)));
            if self.kid {
                // (busy building or in a fight: that's what they're up to)
                if !self.aware.iter().any(|a| matches!(a, Aware::Building { .. } | Aware::Fighting)) {
                    v.push(o(1, "what are you up to?", Act::Go(Node::Play)));
                }
                let wish = match ctx.days {
                    0 => "what did Santa bring you?",
                    d if d > 330 => "what did you get for Christmas?",
                    _ => "what do you want for Christmas?",
                };
                v.push(o(2, wish, Act::Go(Node::Wish)));
                if let Some((t, mine)) = topic {
                    v.push(o(4, &mine, Act::Go(Node::Topic(t, 0))));
                }
                v.push(o(3, "do you believe in Santa?", Act::Go(Node::Santa)));
                if !self.indoors && !self.aware.contains(&Aware::Fighting) {
                    v.push(o(5, "snowball fight?", Act::Snowball));
                }
            } else {
                v.push(o(1, "how are you doing?", Act::Go(Node::How)));
                if let Some((t, mine)) = topic {
                    v.push(o(4, &mine, Act::Go(Node::Topic(t, 0))));
                }
                v.push(o(2, if self.visits >= 2 { "remind me, what is it you do?" } else { "what do you do around here?" }, Act::Go(Node::Job)));
                if let Some(k) = self.next_tip() {
                    v.push(o(3, "anything I should know about the village?", Act::Go(Node::Tip(k))));
                }
                if !self.along {
                    v.push(o(5, "walk with me a while?", Act::Invite));
                }
            }
        }
        v.retain(|o| o.tag.is_none_or(|t| !self.asked.contains(&t)));
        v
    }

    /// What follows on from what they just said (a question about it).
    fn follow_ups(&self, node: Node, ctx: &TalkCtx, talk: &Talk, cocoa: bool) -> Vec<Opt> {
        let o = |tag: u8, label: &str, act: Act| Opt { label: label.to_string(), act, reply: None, tag: Some(tag) };
        // said, answered, and back to the questions
        let leaf = |tag: u8, label: &str, reply: &str| Opt { label: label.to_string(), act: Act::Go(Node::Hub), reply: Some(reply.to_string()), tag: Some(tag) };
        let job = self.job();
        match node {
            Node::How => {
                let help = if self.pick_for(15, 10) < 6 { Node::HelpCocoa } else { Node::HelpCheer };
                vec![o(10, "anything I can do to help?", Act::Go(help))]
            }
            Node::HelpCocoa if cocoa => vec![o(18, "here, have mine", Act::GiveCocoa)],
            Node::HelpCocoa => vec![leaf(19, "I'll bring you one next time", "I'll hold you to that!")],
            Node::HelpCheer if cocoa => vec![o(18, "have a cocoa anyway?", Act::GiveCocoa)],
            Node::Job => vec![o(11, job.ask, Act::Go(Node::JobMore)), o(12, job.long, Act::Go(Node::JobYears))],
            Node::JobMore => vec![o(12, job.long, Act::Go(Node::JobYears))],
            Node::Tip(_) => {
                let given = self.asked.iter().filter(|t| (40..60).contains(*t)).count();
                match self.next_tip() {
                    Some(k) if given < 3 => vec![Opt::new("got another?", Act::Go(Node::Tip(k)))],
                    _ => vec![],
                }
            }
            Node::Topic(t, s) => talk.chat_line(t, s + 1, ctx.days).map(|(mine, _)| vec![Opt::new(&mine, Act::Go(Node::Topic(t, s + 1)))]).unwrap_or_default(),
            Node::Play => vec![leaf(13, "can I help?", PLAY[self.pick_for(4, PLAY.len())].1)],
            Node::Wish if xmas_over(ctx) => vec![],
            Node::Wish => vec![o(14, "have you been good this year?", Act::Go(Node::WishGood)), o(15, "I hope you get it!", Act::Go(Node::WishHope))],
            Node::WishGood => vec![o(15, "I hope you get it!", Act::Go(Node::WishHope))],
            Node::Santa | Node::SantaLeave | Node::SantaStay if (1..=40).contains(&ctx.days) => {
                vec![o(16, "what will you leave out for him?", Act::Go(Node::SantaLeave)), o(17, "will you stay up to see him?", Act::Go(Node::SantaStay))]
            }
            Node::Aware(i, _) => self.aware_opts(Some(i)),
            Node::Cooking => {
                let reply = if self.kid {
                    "here! this one's only a little bit broken"
                } else {
                    ["of course! take two. no, take three", "go on, before they're all gone!", "here, still warm. careful!"][self.pick_for(9, 3)]
                };
                vec![Opt { label: "could I try one?".into(), act: Act::Cookie, reply: Some(reply.into()), tag: Some(29) }]
            }
            Node::HelpAsk => match self.host.as_ref().and_then(|h| h.task) {
                Some(task) => {
                    let thanks = match (self.kid, self.temper) {
                        (true, _) => "yay! thank you!",
                        (_, Temper::Warm) => "bless you, dear",
                        (_, Temper::Gruff) => "hmph. …thank you",
                        (_, Temper::Cheeky) => "my hero",
                        _ => "oh, thank you!",
                    };
                    vec![Opt { label: "leave it to me!".into(), act: Act::Task(task), reply: Some(thanks.into()), tag: Some(30) }, leaf(31, "maybe in a bit", "whenever you like")]
                }
                None => vec![],
            },
            _ => vec![],
        }
    }

    /// What they say on reaching `node`.
    fn say(&mut self, node: Node, ctx: &TalkCtx, talk: &Talk) -> String {
        let job = self.job();
        let kid = self.kid;
        match node {
            // (the choices that lead here carry their own answer)
            Node::Hello | Node::Hub | Node::Aware(..) => "mm-hm".into(),
            Node::How => {
                let m = voice(self.temper, kid).mood;
                if m.is_empty() {
                    "good!".into()
                } else {
                    m[self.pick_for(2, m.len())].to_string()
                }
            }
            Node::HelpCocoa => match self.temper {
                Temper::Warm => "oh, you're a dear. a cocoa would be lovely, if you're passing the cabin".into(),
                Temper::Gruff => "…a cocoa wouldn't hurt. not that I'm asking".into(),
                _ => "honestly? I'd love a cocoa, if you're passing the cabin".into(),
            },
            Node::HelpCheer => "that's kind of you. no, I'm fine. just keep spreading the cheer".into(),
            Node::Job => job.says.into(),
            Node::JobMore => job.more.into(),
            Node::JobYears => job.years.into(),
            Node::Tip(k) => TIPS[k % TIPS.len()].into(),
            Node::Topic(t, s) => talk.chat_line(t, s, ctx.days).map(|(_, theirs)| theirs).unwrap_or_else(|| "hm, I've lost my train of thought".into()),
            Node::Play => PLAY[self.pick_for(4, PLAY.len())].0.into(),
            Node::Wish => {
                let k = self.pick_for(6, WISHES.len());
                if xmas_over(ctx) {
                    GOT[k].into()
                } else {
                    format!("{}!", WISHES[k])
                }
            }
            Node::WishGood => GOOD[self.pick_for(8, GOOD.len())].into(),
            Node::WishHope => "me too! I've got my fingers crossed. and my toes".into(),
            Node::Santa => match ctx.days {
                0 => "he CAME! he ate all the cookies and everything!".into(),
                1 => "of COURSE! he's coming TONIGHT!".into(),
                d @ 2..=40 => format!("of COURSE! only {d} more sleeps!"),
                d if d > 330 => "of course! he came, didn't he? I've got the presents to prove it".into(),
                _ => "of course! I heard his sleigh bells once. I'm sure I did".into(),
            },
            Node::SantaLeave => "cookies for him and carrots for the reindeer!".into(),
            Node::SantaStay => "I'm going to try! I always fall asleep, though".into(),
            Node::HomeLovely => {
                let h = self.host.as_ref();
                use super::interior::Theme as T;
                if h.is_some_and(|h| h.church) {
                    return "three hundred years of candles and carols. the stones are warm with it".into();
                }
                if kid {
                    return ["it's the BEST house. my room's the best bit", "thanks! I helped do the tree. the bottom half"][self.pick_for(17, 2)].into();
                }
                let pride = match h.map(|h| h.theme) {
                    Some(T::Baker) => "it always smells of cinnamon. I've stopped noticing; visitors never do",
                    Some(T::Toymaker) => "mind the tin soldiers. they get everywhere. half of them are \"in testing\"",
                    Some(T::Bookworm) => "every shelf's a different winter. the one by the fire is all ghost stories",
                    Some(T::Musician) => "there's always music in this house. not always good music, mind",
                    Some(T::Sailor) => "every map on the wall is somewhere I've sailed. the telescope still finds the North Star",
                    Some(T::Candles) => "I made every candle in here. forty-two, at the last count. forty-three",
                    Some(T::Farmer) => "my grandfather built that rocking chair. it still creaks his name",
                    None => "it's not much, but it's warm",
                };
                format!("thank you! {pride}")
            }
            Node::Family => {
                let fam = self.host.as_ref().map(|h| h.family.clone()).unwrap_or_default();
                match fam.as_slice() {
                    [a] => format!("that's {a}. the best thing that ever happened to this house"),
                    [a, b] => format!("{a} and {b}. noisy, wonderful, always hungry"),
                    [a, rest @ ..] => format!("{a}, and {} more. noisy, wonderful, always hungry", rest.len()),
                    [] => "it's just me these days. well, me and the whole village".into(),
                }
            }
            Node::Years => {
                let h = self.host.as_ref();
                let y = h.map(|h| h.years).unwrap_or(10);
                if h.is_some_and(|h| h.cabin) {
                    format!("{y} years this winter. I've never once run out of marshmallows. nearly, once")
                } else if h.is_some_and(|h| h.church) {
                    format!("{y} years this winter. I know every creak in the floor")
                } else if kid {
                    "forever! well, since I was a baby".into()
                } else {
                    let memory = [
                        "the winter the river froze, we skated to the bakery",
                        "our first Christmas here, a bird nested in the chimney. it lives in the shed now",
                        "every year the tree gets a bit bigger, and so do the kids",
                        "I still remember the first snow. I cried a bit. happy crying",
                    ][self.pick_for(11, 4)];
                    format!("{y} years this winter. {memory}")
                }
            }
            Node::Cooking => {
                if kid {
                    "gingerbread! I'm allowed the broken ones".into()
                } else {
                    ["gingerbread! my mother's recipe, with a pinch of black pepper", "mince pies. the first batch always goes wrong; the second's for guests", "spiced biscuits. they're for Santa. officially"][self.pick_for(12, 3)].into()
                }
            }
            Node::PetTalk => match self.host.as_ref().and_then(|h| h.pet.clone()) {
                Some((k, n)) => match k {
                    super::pets::PetKind::Dog => format!("that's {n}! greets everyone like they've been away at sea for a year"),
                    super::pets::PetKind::Cat => format!("that's {n}. owns the house, really. we just keep it warm"),
                    super::pets::PetKind::Bunny => format!("that's {n}! does a little twirl when happy"),
                    super::pets::PetKind::Reindeer => format!("that's {n}! one day, the sleigh. for now, carrots"),
                    super::pets::PetKind::Fox => format!("that's {n}. came for the scraps, stayed for the fire"),
                    super::pets::PetKind::Bird => format!("that's {n}! sings along to the radio"),
                },
                None => "just us, I'm afraid".into(),
            },
            Node::HelpAsk => {
                use super::interior::Task as K;
                let pet = self.host.as_ref().and_then(|h| h.pet.as_ref().map(|p| p.1.clone())).unwrap_or_else(|| "the cat".into());
                match (self.host.as_ref().and_then(|h| h.task), kid) {
                    (Some(K::Star), false) => "oh, would you? the star keeps slipping off the top of the tree".into(),
                    (Some(K::Star), true) => "can you put the star back on the tree? I can't reach".into(),
                    (Some(K::Log), false) => "the fire's getting low. could you put another log on?".into(),
                    (Some(K::Log), true) => "the fire's going out! can you put a log on? I'm not allowed".into(),
                    (Some(K::FindPet), false) => format!("{pet} is hiding somewhere again. could you find them?"),
                    (Some(K::FindPet), true) => format!("{pet} is hiding! I've looked EVERYWHERE. can you find them?"),
                    (Some(K::Tune), false) => "the piano's lonely. would you play it something?".into(),
                    (Some(K::Tune), true) => "can you play the piano? I only know one song".into(),
                    (Some(K::Stocking), false) => "there's one stocking left to hang on the mantel…".into(),
                    (Some(K::Stocking), true) => "can you hang up my stocking? it's the red one".into(),
                    (None, false) => "oh, you're sweet. everything's done. just enjoy yourself".into(),
                    (None, true) => "nope! you can just hang out".into(),
                }
            }
            Node::CabinGood => "the cocoa, always. at Christmas the marshmallows are on the house".into(),
            Node::CabinBusy => "every table's full once the lights come on. I wouldn't have it any other way".into(),
            Node::CabinCocoa => (if kid { "SO good. I've got a marshmallow moustache" } else { "best in the village. this is my second cup" }).into(),
            Node::CabinOften => (if kid { "every day, if mum says yes" } else { "every day. it's the warmest spot in town" }).into(),
            Node::Bye => self.rng.pick(voice(self.temper, kid).bye).to_string(),
        }
    }

    /// Pick option `n` (1-based). Returns (what the player said, what they
    /// said back, the act), or None if there's no such option.
    pub fn choose(&mut self, n: usize, t: f32, ctx: &TalkCtx, talk: &Talk, cocoa: bool) -> Option<(String, String, Act)> {
        if self.ended_at.is_some() {
            return None;
        }
        let opt = self.opts.get(n.checked_sub(1)?)?;
        let (mine, act, reply, otag) = (opt.label.clone(), opt.act, opt.reply.clone(), opt.tag);
        if let Act::Pet(_) = act {
            // (the scene builds the next page of the pet's menu)
            self.steps += 1;
            return Some((mine, String::new(), act));
        }
        if let Some(tag) = otag {
            self.asked.push(tag);
        }
        self.steps += 1;
        let node = match act {
            Act::Go(node) => node,
            Act::Invite | Act::Snowball | Act::Task(_) | Act::Pet(_) => Node::Bye,
            Act::GiveCocoa | Act::Cookie | Act::PlayTune | Act::Sit => Node::Hub,
            // after helping, there may be more to say about it
            Act::HelpBuild => match otag {
                Some(tag) if tag >= 100 => Node::Aware(((tag - 100) / 10) as usize, 0),
                _ => Node::Hub,
            },
        };
        if let Node::Tip(k) = node {
            self.asked.push(40 + k as u8);
        }
        let (kid, temper) = (self.kid, self.temper);
        let host = self.host.as_ref().map(|h| (h.cabin, h.keeper, h.church));
        let theirs = match (reply, act) {
            (Some(r), _) => r,
            (None, Act::Invite) => match temper {
                Temper::Warm => "I'd love to, dear. lead the way",
                Temper::Chatty => "ooh, yes! I'll tell you all about my aunt on the way",
                Temper::Shy => "oh. really? okay. I'd like that",
                Temper::Cheeky => "I thought you'd never ask",
                Temper::Dreamy => "yes… let's wander",
                Temper::Gruff => "hmph. alright. not too fast, mind. my knees",
            }
            .to_string(),
            (None, Act::Snowball) => match temper {
                Temper::Shy => "um… okay! not too hard, though",
                Temper::Cheeky => "you're going to regret that! THREE!",
                Temper::Dreamy => "okay! I'll be on my own team!",
                _ => "you're ON! ready? THREE!",
            }
            .to_string(),
            (None, Act::GiveCocoa) => match temper {
                Temper::Warm => "oh, you're a treasure! thank you, dear",
                Temper::Gruff => "…for me? well. thank you. really",
                Temper::Shy => "oh! for me? th-thank you",
                _ => "oh, how kind! thank you!",
            }
            .to_string(),
            (None, Act::HelpBuild) => "thanks!".to_string(),
            (None, Act::Sit) => match host {
                Some((true, true, _)) => "of course! pull up a chair".to_string(),
                Some((true, false, _)) => "go ahead, it's free".to_string(),
                Some((_, _, true)) => "of course. stay as long as you like".to_string(),
                _ if kid => "sure! not that one, that's grandpa's".to_string(),
                _ => ["please! make yourself at home", "of course. the comfy one's free", "sit, sit! you're a guest"][self.pick_for(13, 3)].to_string(),
            },
            (None, Act::PlayTune) => {
                if kid {
                    "I only know one song! here goes…".to_string()
                } else {
                    ["oh, go on then", "just the one! …or two", "this one's my favourite"][self.pick_for(14, 3)].to_string()
                }
            }
            (None, Act::Cookie) => "here you are!".to_string(),
            (None, Act::Task(_)) => "thank you!".to_string(),
            (None, Act::Pet(_)) => String::new(),
            (None, Act::Go(_)) => self.say(node, ctx, talk),
        };
        self.line = theirs.clone();
        self.opts = self.options(node, ctx, talk, cocoa);
        if self.opts.is_empty() {
            self.ended_at = Some(t);
        }
        Some((mine, theirs, act))
    }

    /// Where the box goes on screen: (x0, y0, width, height) and the
    /// wrapped lines of what they said.
    fn layout(&self, w: i32, h: i32) -> (i32, i32, i32, i32, Vec<String>) {
        let bw = (w - 4).clamp(24, 66);
        let said = wrap(&self.line, (bw - 4) as usize);
        let bh = 2 + said.len() as i32 + if self.opts.is_empty() { 0 } else { 1 + self.opts.len() as i32 };
        let (x0, y0) = match self.pos {
            Some((x, y)) => (x.clamp(0, (w - bw).max(0)), y.clamp(0, (h - bh).max(0))),
            None => ((w - bw) / 2, (h - bh - 1).max(0)),
        };
        (x0, y0, bw, bh, said)
    }

    /// Option under (x, y) on screen, if any (1-based).
    pub fn option_at(&self, x: i32, y: i32, w: i32, h: i32) -> Option<usize> {
        let (x0, y0, bw, _, said) = self.layout(w, h);
        let first = y0 + 1 + said.len() as i32 + 1;
        let i = y - first;
        (x > x0 && x < x0 + bw - 1 && i >= 0 && (i as usize) < self.opts.len()).then_some(i as usize + 1)
    }

    /// The box's top-left corner on screen.
    pub fn origin(&self, w: i32, h: i32) -> (i32, i32) {
        let (x0, y0, ..) = self.layout(w, h);
        (x0, y0)
    }

    pub fn contains(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let (x0, y0, bw, bh, _) = self.layout(w, h);
        x >= x0 && x < x0 + bw && y >= y0 && y < y0 + bh
    }

    pub fn draw(&self, f: &mut Frame, hover: Option<usize>, look: &str) {
        let (w, h) = (f.w as i32, f.h as i32);
        let (x0, y0, bw, bh, said) = self.layout(w, h);
        let bg = Bg::Mix(pal::UI_PANEL, 0.94);
        self.draw_box(f, hover, x0, y0, bw, bh, &said, bg);
        crate::widgets::looks::apply(f, crate::widgets::Rect { x: x0, y: y0, w: bw, h: bh }, look, bg, 9);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_box(&self, f: &mut Frame, hover: Option<usize>, x0: i32, y0: i32, bw: i32, bh: i32, said: &[String], bg: Bg) {
        let border = Rgb::hex(0xffd36b) * 0.8;
        for y in y0..y0 + bh {
            for x in x0..x0 + bw {
                let ch = match (x == x0, x == x0 + bw - 1, y == y0, y == y0 + bh - 1) {
                    (true, _, true, _) => '╭',
                    (_, true, true, _) => '╮',
                    (true, _, _, true) => '╰',
                    (_, true, _, true) => '╯',
                    (_, _, true, _) | (_, _, _, true) => '─',
                    (true, _, _, _) | (_, true, _, _) => '│',
                    _ => ' ',
                };
                f.put_g(x, y, Glyph::new(ch, border).bg(bg).lit(0.0));
            }
        }
        let title = format!(" {} ", self.name);
        f.text(x0 + 2, y0, &title, pal::UI_TITLE, bg, 0.0);
        // a grip: the box can be dragged anywhere
        f.text(x0 + bw - 5, y0, " ⠿ ", pal::UI_DIM, bg, 0.0);
        for (i, l) in said.iter().enumerate() {
            f.text(x0 + 2, y0 + 1 + i as i32, l, Rgb::hex(0xf4ecd8), bg, 0.0);
        }
        let first = y0 + 1 + said.len() as i32 + 1;
        for (i, o) in self.opts.iter().enumerate() {
            let y = first + i as i32;
            let hot = hover == Some(i + 1);
            let fg = if hot { Rgb::hex(0xffd36b) } else { pal::UI_TEXT };
            if hot {
                f.text(x0 + 1, y, "▸", Rgb::hex(0xffd36b), bg, 0.0);
            }
            f.text(x0 + 2, y, &format!("{}", i + 1), Rgb::hex(0xffd36b), bg, 0.0);
            let label: String = o.label.chars().take((bw - 8) as usize).collect();
            f.text(x0 + 5, y, &label, fg, bg, 0.0);
        }
        if !self.opts.is_empty() {
            let foot = " ↑↓ Enter · 1-5 · click to answer · Esc to leave ";
            f.text(x0 + bw - 2 - foot.chars().count() as i32, y0 + bh - 1, foot, pal::UI_DIM, bg, 0.0);
        }
    }
}

/// Word-wrap to `w` columns.
fn wrap(s: &str, w: usize) -> Vec<String> {
    let mut out = vec![];
    let mut line = String::new();
    for word in s.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > w {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;
    use super::super::dialogue::Part;

    /// Every situation worth walking through: (context, what they're aware
    /// of; building and snowball fights are kids' things).
    fn situations(seed: u32, kid: bool) -> (TalkCtx, Vec<Aware>) {
        let part = [Part::Night, Part::Morning, Part::Day, Part::Evening][seed as usize % 4];
        let days = [9, 1, 0, 100, 360, 3][seed as usize % 6];
        let ctx = TalkCtx { part, days, train: seed % 3 == 0, snowy: seed % 5 == 0, show: seed % 7 == 0, stars: seed % 4 == 1, ..Default::default() };
        let aware = [
            vec![],
            vec![Aware::Building { igloo: seed % 3 == 0 }, Aware::SawFeed("bunny")],
            vec![Aware::GotHit, Aware::Fighting],
            vec![Aware::WasCarried, Aware::SawDropIn, Aware::Built { igloo: false }],
            vec![Aware::HasPet(super::super::pets::PetKind::Dog, "Biscuit"), Aware::SawGift],
            vec![Aware::TalkedToSanta, Aware::SawVisit, Aware::SeeBuild { igloo: true }],
        ][seed as usize % 6]
            .clone();
        let aware = aware.into_iter().filter(|a| kid || !matches!(a, Aware::Building { .. } | Aware::Built { .. } | Aware::Fighting)).collect();
        (ctx, aware)
    }

    /// No two choices the same (or one inside another, like "what are you
    /// up to?" and "hi! what are you up to?").
    fn assert_distinct(opts: &[Opt]) {
        for (i, a) in opts.iter().enumerate() {
            for b in &opts[i + 1..] {
                let (x, y) = (a.label.to_lowercase(), b.label.to_lowercase());
                assert!(x != y && !(x.len() > 8 && y.contains(&x)) && !(y.len() > 8 && x.contains(&y)), "{:?}", opts.iter().map(|o| &o.label).collect::<Vec<_>>());
            }
        }
    }

    /// Walk every path a little way: there's always something to say until
    /// goodbye, nothing's left unfilled or said twice, a question is never
    /// answered with a question, and goodbye ends it.
    #[test]
    fn conversations_branch_and_end() {
        for kid in [false, true] {
            for seed in 0..120u32 {
                let (ctx, aware) = situations(seed, kid);
                let mut talk = Talk::new(seed);
                let mut c = Convo::open(seed as u64 + 1, "Martha", kid, seed % 5, &ctx, &mut talk, 0.0, seed, aware);
                assert!(!c.line.is_empty());
                let mut rng = Rng::new(seed as u64);
                let mut said = vec![c.line.clone()];
                for step in 0..14 {
                    if c.ended_at.is_some() {
                        break;
                    }
                    assert!(!c.opts.is_empty() && c.opts.len() <= 5, "{:?}", c.opts.len());
                    assert_distinct(&c.opts);
                    // mostly not goodbye, so the tree gets explored
                    let n = if step < 12 { rng.int(1, c.opts.len().max(2) as i32) as usize } else { c.opts.len() };
                    let (mine, theirs, _) = c.choose(n, step as f32, &ctx, &talk, seed % 2 == 0).expect("valid option");
                    assert!(!mine.is_empty() && !theirs.is_empty());
                    assert!(!theirs.contains('{') && !mine.contains('{'), "unfilled: {mine} / {theirs}");
                    assert!(!(mine.ends_with('?') && theirs.ends_with('?')), "a question for a question: {mine} / {theirs}");
                    assert!(!said.contains(&theirs), "said twice: {theirs}");
                    said.push(theirs);
                }
                let n = c.opts.len();
                if n > 0 {
                    c.choose(n, 20.0, &ctx, &talk, false);
                }
                assert!(c.ended_at.is_some(), "goodbye didn't end it");
            }
        }
    }

    /// Hellos are to the villager (not the whole village), and don't say
    /// it's night in the daytime.
    #[test]
    fn hellos_fit_the_moment() {
        for kid in [false, true] {
            for seed in 0..200u32 {
                let ctx = TalkCtx { part: Part::Day, days: 100, ..Default::default() };
                let mut talk = Talk::new(seed);
                let c = Convo::open(seed as u64 + 7, "Henry", kid, seed % 5, &ctx, &mut talk, 0.0, seed, vec![]);
                let l = c.line.to_lowercase();
                assert!(!l.contains(", everyone") && !l.contains("village!") && !l.contains("night") && !l.contains("evening"), "{}", c.line);
            }
        }
    }

    /// A kid offers one snowball fight, not two; the questions always on
    /// offer don't turn up again as a topic.
    #[test]
    fn no_question_twice() {
        let ctx = TalkCtx { snowy: true, train: true, days: 9, ..Default::default() };
        for seed in 0..60u32 {
            let mut talk = Talk::new(seed);
            let c = Convo::open(seed as u64 + 3, "Pip", true, 1, &ctx, &mut talk, 0.0, seed, vec![]);
            assert!(c.opts.iter().filter(|o| o.label.contains("snowball")).count() <= 1);
            assert_distinct(&c.opts);
        }
    }

    /// Something on their mind that they didn't open with is brought up,
    /// not answered as if they had ("I saw!" to a question never asked).
    #[test]
    fn what_they_noticed_makes_sense_either_way() {
        use super::super::pets::PetKind;
        let all = [
            Aware::Building { igloo: false },
            Aware::Building { igloo: true },
            Aware::Built { igloo: false },
            Aware::Built { igloo: true },
            Aware::SeeBuild { igloo: false },
            Aware::Fighting,
            Aware::SawFeed("deer"),
            Aware::SawPet("fox"),
            Aware::SawSnowball,
            Aware::GotHit,
            Aware::SawGift,
            Aware::GotGift,
            Aware::WasCarried,
            Aware::SawCarry,
            Aware::SawDropIn,
            Aware::SawVisit,
            Aware::TalkedToSanta,
            Aware::HasPet(PetKind::Cat, "Mittens"),
        ];
        for a in all {
            for kid in [false, true] {
                let (opener, said) = a.lines(kid, 3, true);
                let (_, unsaid) = a.lines(kid, 3, false);
                assert!(!unsaid.is_empty());
                // (a question opener is answered by the first line; without it,
                // that line has to stand on its own)
                if opener.as_deref().is_some_and(|o| o.contains('?')) {
                    assert_ne!(said[0].0, unsaid[0].0, "{a:?}");
                }
                for (label, reply, _) in &unsaid {
                    assert!(!label.starts_with("I saw") && !label.starts_with("I did"), "{a:?}: {label}");
                    assert!(!(label.ends_with('?') && reply.ends_with('?')), "{a:?}: {label} / {reply}");
                }
            }
        }
    }

    /// "got another?" never gives the same tip twice.
    #[test]
    fn tips_dont_repeat() {
        let ctx = TalkCtx::default();
        for seed in 0..20u32 {
            let mut talk = Talk::new(seed);
            let mut c = Convo::open(seed as u64 + 11, "Edith", false, 1, &ctx, &mut talk, 0.0, seed, vec![]);
            let k = c.opts.iter().position(|o| matches!(o.act, Act::Go(Node::Tip(_)))).expect("a tip on offer") + 1;
            let mut tips = vec![c.choose(k, 0.0, &ctx, &talk, false).unwrap().1];
            while let Some(k) = c.opts.iter().position(|o| o.label == "got another?") {
                tips.push(c.choose(k + 1, 0.0, &ctx, &talk, false).unwrap().1);
            }
            let uniq: std::collections::HashSet<&String> = tips.iter().collect();
            assert!(tips.len() >= 2 && uniq.len() == tips.len(), "{tips:?}");
        }
    }

    /// In the cocoa cabin nobody's asked about their home or family; kids at
    /// home haven't lived there forty years.
    #[test]
    fn hosts_talk_about_where_they_are() {
        use super::super::interior::Theme;
        let ctx = TalkCtx::default();
        let h = |cabin: bool, keeper: bool| HostCtx { theme: Theme::Baker, family: vec!["Pip"], pet: None, years: 40, church: false, piano: false, oven: true, task: None, cabin, keeper };
        for (cabin, keeper) in [(true, true), (true, false)] {
            let mut talk = Talk::new(1);
            let c = Convo::host(9, "Otto", false, Temper::Warm, 0, &ctx, &mut talk, h(cabin, keeper), 1);
            assert!(c.opts.iter().all(|o| !o.label.contains("home") && !o.label.contains("family")), "{:?}", c.opts.iter().map(|o| &o.label).collect::<Vec<_>>());
        }
        let mut talk = Talk::new(1);
        let mut c = Convo::host(10, "Dot", true, Temper::Chatty, 0, &ctx, &mut talk, h(false, false), 1);
        let k = c.opts.iter().position(|o| o.act == Act::Go(Node::Years)).unwrap() + 1;
        let said = c.choose(k, 0.0, &ctx, &talk, false).unwrap().1;
        assert!(!said.contains("40"), "{said}");
    }

    #[test]
    fn the_same_person_has_the_same_job() {
        let ctx = TalkCtx::default();
        let mut talk = Talk::new(1);
        let job = |talk: &mut Talk| {
            let mut c = Convo::open(42, "Henry", false, 1, &ctx, talk, 0.0, 7, vec![]);
            let k = c.opts.iter().position(|o| o.act == Act::Go(Node::Job)).unwrap() + 1;
            c.choose(k, 0.0, &ctx, talk, false).unwrap().1
        };
        assert_eq!(job(&mut talk), job(&mut talk));
    }

    #[test]
    fn wraps_long_lines() {
        let w = wrap("the quick brown fox jumps over the lazy dog", 12);
        assert!(w.iter().all(|l| l.chars().count() <= 12));
        assert_eq!(w.join(" "), "the quick brown fox jumps over the lazy dog");
    }
}
