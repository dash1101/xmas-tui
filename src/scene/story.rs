//! Talking with the villagers, story-game style: a dialogue box with what
//! they said and a few things to say back (1-5, or click one). Every
//! grown-up has a job and a mood, every kid a wish and something they're
//! up to — the same each time for the same person — and the talk can wander
//! onto whatever's going on in the village. Some choices do things: ask
//! someone to walk along, hand over a cocoa, start a snowball fight.

use super::dialogue::{Talk, TalkCtx};
use crate::color::{pal, Rgb};
use crate::frame::{Bg, Frame, Glyph};
use crate::rng::{hash, Rng};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Node {
    Hello,
    Again,
    How,
    Help,
    Job,
    JobMore,
    JobYears,
    Tip,
    Topic(usize, usize),
    Play,
    PlayHelp,
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
    Bye,
}

/// What a host at home talks about (from the household).
#[derive(Clone, Debug)]
pub struct HostCtx {
    pub theme: super::interior::Theme,
    pub family: Vec<&'static str>,
    pub pet: Option<(super::pets::PetKind, String)>,
    pub years: u32,
    pub church: bool,
    pub piano: bool,
    pub oven: bool,
    /// The job they'd ask for help with (picked when asked).
    pub task: Option<super::interior::Task>,
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
    /// Talked with Santa from the ground just now.
    TalkedToSanta,
    /// Out with their pet (kind, its name).
    HasPet(super::pets::PetKind, &'static str),
}

impl Aware {
    /// What they open with (if it's on their mind), and the things that can
    /// be said about it: (what the villager says, the reply, what it does).
    fn lines(self, kid: bool, pick: usize) -> (Option<String>, Vec<(String, String, Option<Act>)>) {
        let l = |a: &str, b: &str| (a.to_string(), b.to_string(), None);
        let act = |a: &str, b: &str, x: Act| (a.to_string(), b.to_string(), Some(x));
        let names = ["Sir Snowsalot", "Frosty Junior", "Mister Carrots", "Big Steve", "Captain Chilly"];
        match self {
            Aware::Building { igloo: false } => (
                Some("we're building a snowman!! wanna help?".into()),
                vec![
                    l("that's a great start on the snowman!", "it's gonna be the BIGGEST one ever"),
                    act("need a hand with it?", "YES! you pack, I'll roll!", Act::HelpBuild),
                    l("what will you call him?", &format!("{}!", names[pick % names.len()])),
                ],
            ),
            Aware::Building { igloo: true } => (
                Some("we're making an igloo! it's gonna be HUGE".into()),
                vec![
                    l("how's the igloo coming along?", "slowly. the blocks keep falling over"),
                    act("can I help build it?", "grab some blocks! pack 'em tight!", Act::HelpBuild),
                    l("will I fit inside?", "hmm… if you squeeze. maybe. no."),
                ],
            ),
            Aware::Built { igloo: false } => (
                Some("did you see our snowman?! we MADE it!".into()),
                vec![l("your snowman's brilliant!", "thanks!! the hat was MY idea"), l("does he have a name?", &format!("{}! say hi to him!", names[pick % names.len()]))],
            ),
            Aware::Built { igloo: true } => (
                Some("we built an IGLOO! did you see?".into()),
                vec![l("how is it inside the igloo?", "SO cozy. members only though"), l("can I join the igloo club?", "you have to know the secret knock")],
            ),
            Aware::SeeBuild { igloo } => {
                let thing = if igloo { "igloo" } else { "snowman" };
                if kid {
                    (None, vec![l(&format!("did you build that {thing}?"), "no, but I helped! a bit. I watched")])
                } else {
                    (None, vec![l(&format!("who built that {thing}?"), "the kids! took them all afternoon. isn't it grand?")])
                }
            }
            Aware::Fighting => (
                Some("can't talk — SNOWBALL FIGHT!!".into()),
                vec![l("who's winning?", "US! obviously!"), act("can I join in?", "you're on OUR team! …just kidding — INCOMING!", Act::Snowball)],
            ),
            Aware::SawFeed(a) => (
                Some(if kid { format!("you fed the {a}!! that was SO cool") } else { format!("that {a} has taken a shine to you, I saw") }),
                vec![if kid { l(&format!("did you see the {a} eat?"), "from your HAND! can I try next time?") } else { l(&format!("did you see me feed that {a}?"), "I did! it looked so pleased. you've a way with animals") }],
            ),
            Aware::SawPet(a) => (
                None,
                vec![if kid { l(&format!("did you see me pet the {a}?"), "it let you!! they never let me") } else { l(&format!("that {a} was friendly, wasn't it?"), "they're tame as anything round here, if you're gentle") }],
            ),
            Aware::SawSnowball => (
                None,
                vec![if kid { l("nice throw, huh?", "it was okay. I could do better") } else { l("sorry if that snowball came close", "ha! I've thrown a few in my time") }],
            ),
            Aware::GotHit => (
                Some(if kid { "hey! you got me with that snowball!".into() } else { "you got me with a snowball, you rascal!".into() }),
                if kid {
                    vec![l("sorry! couldn't resist", "the next one's coming YOUR way"), act("rematch?", "oh it is SO on", Act::Snowball)]
                } else {
                    vec![l("sorry! couldn't resist", "ha! I'd have done the same at your age")]
                },
            ),
            Aware::SawGift => (None, vec![l("did you see me hand out a present?", if kid { "can I have one?? please?" } else { "I did — that was lovely of you" })]),
            Aware::GotGift => (
                Some(if kid { "thank you for the present!! I'm saving it". into() } else { "thank you again for the gift — so thoughtful".into() }),
                vec![l("I hope you like it!", if kid { "I LOVE it. I'm not sharing it" } else { "it's perfect. I'll treasure it" })],
            ),
            Aware::WasCarried => (
                Some(if kid { "do the flying thing again!!".into() } else { "you carried me halfway across the village!".into() }),
                vec![l("sorry about carrying you around", if kid { "again! again! AGAIN!" } else { "my feet never touched the ground. I'm still dizzy" })],
            ),
            Aware::SawCarry => (None, vec![l("did you see me carrying someone?", if kid { "can you carry ME next?!" } else { "I did! they looked a little dizzy afterwards" })]),
            Aware::SawDropIn => (
                Some(if kid { "did you just fall out of the SKY?!".into() } else { "did you just… come down from the sky?".into() }),
                vec![l("I came down from the sky, you know", if kid { "teach me! teach me!" } else { "I saw! I nearly dropped my cocoa" })],
            ),
            Aware::SawVisit => (None, vec![l("I was just visiting next door", if kid { "did they give you cookies?" } else { "they make the best mince pies, don't they?" })]),
            Aware::HasPet(k, n) => {
                let fact = match k {
                    super::pets::PetKind::Dog => ["he'd follow me to the North Pole", "she's never met a puddle she didn't like", "he's three, but he thinks he's a reindeer"][pick % 3],
                    super::pets::PetKind::Cat => ["she only came because it's sunny somewhere", "he walks me, really", "she's judging your boots. affectionately"][pick % 3],
                    super::pets::PetKind::Bunny => ["she does a little jump when she's happy", "he's braver than he looks", "she's had four carrots today. it's a holiday"][pick % 3],
                };
                (
                    None,
                    vec![
                        l("and who's this little one?", &format!("this is {n}! {fact}")),
                        l(&format!("can I give {n} a fuss?"), &format!("of course! {n} adores a fuss")),
                    ],
                )
            }
            Aware::TalkedToSanta => (
                Some(if kid { "I talked to SANTA!! did you see?!".into() } else { "I just spoke to Santa! can you believe it?".into() }),
                vec![l("you talked to Santa!", if kid { "he said I'm on the nice list!! I think" } else { "he remembered my name. after all these years" })],
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
    /// What they say back (for answers that aren't a plain step).
    reply: Option<String>,
    /// Marks the answer as taken (for awareness lines).
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
    /// Branches already taken (not offered again).
    asked: Vec<u8>,
    /// Who they are (stable per person).
    seed: u32,
    topic: Option<usize>,
    /// What they're aware of (most on their mind first).
    aware: Vec<Aware>,
    /// Where the box was dragged to (top-left), if it was.
    pub pos: Option<(i32, i32)>,
    /// How they talk (the same each time for the same person).
    pub temper: Temper,
    /// Talking with someone in their own home.
    pub host: Option<HostCtx>,
    rng: Rng,
}

/// (job, what it's like, a bit more)
const JOBS: &[(&str, &str, &str)] = &[
    ("baker", "up at four every morning for the gingerbread", "the secret's a pinch of black pepper. don't tell a soul"),
    ("stationmaster", "I keep the trains running on time. mostly", "the 6:15 has never once been late. the 6:40, well…"),
    ("schoolteacher", "the kids are a handful this close to Christmas", "we're doing a nativity play. three of them want to be the donkey"),
    ("carpenter", "I made half the sleds in this village", "the trick is waxing the runners. goes like the wind"),
    ("candle maker", "every window light you see? probably one of mine", "cinnamon's the best seller. pine for the traditionalists"),
    ("postmaster", "letters to Santa all month — sacks of them", "I read none of them. officially"),
    ("choir leader", "we sing at the church on Christmas Eve", "we could use another tenor, if you're asking"),
    ("ice fisher", "out on the lake most mornings", "caught a trout THIS big once. well — this big"),
    ("toy maker", "wooden trains, mostly. and the odd rocking horse", "between us, Santa buys wholesale"),
    ("retired sailor", "forty years at sea; now I just watch the snow", "nothing's as quiet as a snowy night. not even the ocean"),
    ("librarian", "it's warm, it's quiet, and it's full of stories", "come by for a Christmas story some evening"),
    ("farmer", "the cows don't care that it's Christmas", "they get extra hay on the day, though. tradition"),
];



const TIPS: &[&str] = &[
    "the moon likes being clicked. don't ask me why",
    "type 'hohoho' when nobody's listening — someone up north might hear",
    "berries from the bushes make the deer your friends",
    "Ctrl and the scroll wheel take you right up past the clouds",
    "give a bush a shake and see what falls out",
    "the cocoa cabin sells lanterns — handy after dark",
    "knock on a door; most folk will let you in",
    "there are pictures in the stars up high, if you look",
    "the power lines by the hills… best leave those alone",
];


const PLAY: &[&str] = &["building the BIGGEST snowman", "looking for reindeer tracks", "counting the lights on the big tree — I'm at 300!", "practising my snowball throw", "catching snowflakes on my tongue", "making snow angels. I've made eleven"];
const PLAY_HELP: &[&str] = &["yes!! you can be on my team", "okay, but I'm the boss", "you can hold my mittens", "only if you do the heavy bits"];
const WISHES: &[&str] = &["a sled that goes SUPER fast", "a puppy. a real one", "a telescope, to watch for Santa", "a drum kit! mum says no", "a dinosaur. a small one", "snow that never ever melts"];
const GOOD: &[&str] = &["SO good. mostly. okay — a little bit good", "I tidied my room! well, I pushed it all under the bed", "I shared my cookies. one of them", "I've been the goodest"];

/// How someone talks. Everyone has one (the same each time), and it colours
/// their hellos, how they're doing, what they say between topics and how
/// they say goodbye.
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
/// again to a regular, how they are, between topics, goodbye).
struct Voice {
    hi: &'static [&'static str],
    again: &'static [&'static str],
    regular: &'static [&'static str],
    mood: &'static [&'static str],
    more: &'static [&'static str],
    bye: &'static [&'static str],
}

fn voice(t: Temper, kid: bool) -> Voice {
    match (t, kid) {
        (Temper::Warm, _) => Voice {
            hi: &["oh, hello, dear! I'm {name} — welcome to the village!", "well hello there! I'm {name}. you look frozen — come stand by the lamp", "a new face! I'm {name}, dear. merry Christmas!"],
            again: &["there you are! I was hoping you'd stop by", "hello again, dear! *brushes the snow off your shoulder*", "oh, it's you! come here, you're letting the cold in your collar"],
            regular: &["my favourite neighbour! how ARE you?", "*beams* back again! you're always welcome, you know"],
            mood: &["all the better for seeing you, dear", "warm as toast — I've had two cocoas and a mince pie", "a little tired, but the good kind of tired"],
            more: &["what else, dear?", "go on, I've got all the time in the world", "anything else on your mind, sweetheart?"],
            bye: &["off you go — and wrap up warm, dear!", "come by for cocoa anytime. I mean it!", "mind the ice, now. merry Christmas!"],
        },
        (Temper::Chatty, false) => Voice {
            hi: &["hi! hello! I'm {name}! oh, you must be new — isn't it all so LOVELY?", "oh! hello! {name}, that's me. have you tried the cocoa yet? you must!", "{name}! pleased to meet you! did you see the train? I saw the train"],
            again: &["you again! oh good, I've got news", "hello hello! I was JUST thinking about you", "oh! perfect timing — you'll never guess what happened"],
            regular: &["my favourite listener! sit, sit — well, stand, it's snowing", "you're becoming a regular! I love that"],
            mood: &["oh, wonderful! I've had three cocoas and it's not even dark yet", "busy busy! I've wrapped forty presents. ish. thirty", "fantastic! my aunt's coming tomorrow and she brings fudge"],
            more: &["ooh, what else? ask me anything!", "and THEN? go on!", "I could talk all night, honestly"],
            bye: &["oh, I could talk all night! bye for now!", "wait, one more thing— oh, never mind. merry Christmas!", "bye! come find me, I'll have more news!"],
        },
        (Temper::Shy, false) => Voice {
            hi: &["oh! um… hi. I'm {name}.", "h-hello. I'm {name}. merry Christmas…", "oh — hi. I'm {name}. *pulls scarf up a little*"],
            again: &["oh — hi again… *smiles*", "you came back to say hi? that's… really nice", "oh. hello again. I hoped you would"],
            regular: &["hi! …I mean, hello. it's always nice when you stop by", "*waves a mitten* hi again"],
            mood: &["good, thanks… a little cold", "okay! I found a really nice icicle earlier", "good. better now, actually"],
            more: &["um… anything else?", "you can ask me something else, if you want", "…what else?"],
            bye: &["bye… thanks for talking to me", "merry Christmas. …see you around?", "bye! *waves a little*"],
        },
        (Temper::Cheeky, false) => Voice {
            hi: &["well, well — a new face! {name}, at your service", "{name}. charming, handsome, excellent at snowball fights. you?", "ah, the newcomer! I'm {name}. you'll want to stay on my good side"],
            again: &["back for more of my wisdom?", "ha! couldn't stay away, could you?", "you again! people will start to talk"],
            regular: &["my number one fan returns!", "at this rate I'll have to put you in my will"],
            mood: &["freezing, starving and thrilled about it", "never better. I'm on the nice list. probably", "excellent — I've just hidden a snowball in the postbox"],
            more: &["go on, I've got all night", "what else? I know everyone's secrets", "and? spit it out!"],
            bye: &["don't do anything I wouldn't do!", "off you go. and stay out of the postbox", "toodle-oo! merry Christmas!"],
        },
        (Temper::Dreamy, false) => Voice {
            hi: &["hello… I'm {name}. isn't the snow beautiful tonight?", "oh, hello. I'm {name}. I was just counting snowflakes", "I'm {name}. do you ever think the lights are whispering?"],
            again: &["oh, it's you… hello again", "hello again. the stars are so clear tonight, look", "you're back. I was just thinking of you, and of cinnamon"],
            regular: &["hello, friend. the snow's been kinder since you came", "oh, good. you always arrive at the nicest moments"],
            mood: &["I've been watching the snowflakes. each one's different, you know", "peaceful. like the hush after carols", "a little far away, but the nice kind of far"],
            more: &["what else is on your mind?", "mm… what else?", "tell me something, or ask me something"],
            bye: &["may your night be full of stars", "goodbye for now. listen for the sleigh bells", "take some of the quiet with you"],
        },
        (Temper::Gruff, false) => Voice {
            hi: &["{name}. …welcome, I suppose. mind the ice", "hmph. new, are you? {name}. …merry Christmas", "{name}. don't step on the flowerbed. there's one under there somewhere"],
            again: &["you again. …good. it's nice to see you", "hmph. back, are you. well. hello", "oh, it's you. *almost smiles*"],
            regular: &["you're alright, you know that? don't tell anyone I said so", "the regular. good. the village needs folk like you"],
            mood: &["cold. but… not bad. not bad at all", "my knees know it's December. otherwise fine", "fine. the cocoa helps. don't tell the cabin"],
            more: &["something else?", "well? out with it", "what else, then"],
            bye: &["go on then. …merry Christmas", "off with you. and wear a hat", "hmph. take care of yourself"],
        },
        // kids
        (Temper::Chatty, true) => Voice {
            hi: &["hi!! I'm {name}! who are YOU?", "hiya! I'm {name}! do you like snow? I LOVE snow", "I'm {name}! I'm six! and three quarters!"],
            again: &["it's YOU! hi hi hi!", "you came back!! guess what guess what", "hiii! I was hoping you'd come!"],
            regular: &["you're my best friend now. that's just how it is", "HI! I told everyone about you!"],
            mood: &["GREAT! I found a stick shaped like a reindeer!", "SO good! it's nearly Christmas!!", "amazing! I ate snow. don't tell"],
            more: &["what else what else?", "ask me something else!", "okay what now?"],
            bye: &["bye!! come back soon!!", "BYE! merry Christmas! bye!", "see ya! don't forget me!"],
        },
        (Temper::Shy, true) => Voice {
            hi: &["…hi. I'm {name}.", "oh! um. I'm {name}. *hides behind mitten*", "hi… I'm {name}. I like your hat"],
            again: &["oh! it's you… hi", "*small wave* hi again", "you came back… cool"],
            regular: &["hi! …I saved you a good snowball", "*runs over* hi!"],
            mood: &["good… I made a tiny snowman. he's shy too", "okay! I saw a bunny", "good. my nose is cold"],
            more: &["um… what else?", "…you can ask more stuff", "what else?"],
            bye: &["bye… *waves*", "bye! …thanks for talking to me", "merry Christmas… bye"],
        },
        (Temper::Cheeky, true) => Voice {
            hi: &["I'm {name}! I'm the best snowball thrower in the WORLD", "hi! I'm {name}. bet you can't catch me", "I'm {name}! wanna know a secret? …I'm not telling"],
            again: &["you again! ready to lose a snowball fight?", "ha! you're back! tag, you're it!", "hi! I still have a snowball with your name on it"],
            regular: &["my favourite grown-up! don't tell my mum", "you're back! I've been practising my throw"],
            mood: &["awesome! I hit the postman with a snowball. by accident. mostly", "great! I'm on the nice list. probably", "SO good! I ate an icicle"],
            more: &["what else? quick!", "ask me something hard", "and? and?"],
            bye: &["bye! watch your back! hehe", "see ya, slowpoke!", "bye! merry Christmas! *throws a snowball*"],
        },
        (_, true) => Voice {
            hi: &["hi… I'm {name}. do you think snowflakes can hear us?", "I'm {name}. I'm watching the stars", "hello. I'm {name}. the moon's following me"],
            again: &["oh, it's you! hello", "hi again! look, the snow's sparkly", "you're back! I was just thinking about reindeer"],
            regular: &["hi, friend. I drew a picture of you. in the snow", "hello! you're always here at the best bits"],
            mood: &["good! I'm waiting for a shooting star", "sleepy, but the happy kind", "good. I'm listening for sleigh bells"],
            more: &["what else?", "ask me another thing", "mm… what else?"],
            bye: &["bye… sweet dreams later", "bye! look up at the stars tonight", "merry Christmas… bye"],
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

/// A hello with the villager's name in it (when they've picked one): on a
/// first meeting, now and then word has got round; after that, people
/// greet them by name.
/// A friend's (or family member's) first words: they know the villager.
pub fn friend_hello(you: &str, me: &str, kid: bool, roll: u32) -> String {
    let lines: &[&str] = match (kid, you.is_empty()) {
        (false, false) => &["{you}! so good to see you", "{you}! I was hoping I'd bump into you", "there you are, {you}! isn't the village lovely?", "{you}! it's me, {me}. fancy meeting you here"],
        (true, false) => &["{you}!! it's me, {me}!", "{you}! come and play!", "{you}!! I came to see the big tree!", "{you}! guess what? it's nearly Christmas!"],
        (false, true) => &["there you are! so good to see you", "oh, it's you! hello, old friend", "it's me, {me}! I was hoping I'd see you"],
        (true, true) => &["it's me, {me}! come and play!", "hi!! I came to see the big tree!", "it's you!! hi!!"],
    };
    lines[roll as usize % lines.len()].replace("{you}", you).replace("{me}", me)
}

/// A friend calling out as they come by.
pub fn friend_call(you: &str, kid: bool, roll: u32) -> String {
    let lines: &[&str] = match (kid, you.is_empty()) {
        (false, false) => &["{you}! hello!", "{you}! over here!", "oh, {you}! merry Christmas!"],
        (true, false) => &["{you}!! hi!!", "{you}! {you}! over here!", "hi {you}!!"],
        (false, true) => &["hello, you! over here!", "oh, it's you! merry Christmas!"],
        (true, true) => &["hi!! over here!", "it's you!! hi!!"],
    };
    lines[roll as usize % lines.len()].replace("{you}", you)
}

pub fn by_name(line: &str, you: &str, first: bool, kid: bool, roll: u32) -> String {
    if you.is_empty() {
        return line.to_string();
    }
    match (first, kid, roll % 3) {
        (true, false, 0) => format!("{line} oh — you must be {you}! word gets round in a village this size"),
        (true, true, 0) => format!("{line} wait, are you {you}? everyone's talking about you!"),
        (true, ..) => line.to_string(),
        (false, true, 0 | 1) => format!("{you}!! {line}"),
        (false, false, 0) => format!("{you}! {line}"),
        (false, false, 1) => format!("ah, {you}. {line}"),
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
            pos: None,
            temper: Temper::Warm,
            host: None,
            rng: Rng::new((seed ^ id as u32) as u64 ^ 0x5a1),
        };
        c.temper = temper_of(id, kid);
        // something topical to talk about, if there is anything going on
        let topic = talk.chat_topic(ctx, x, kid);
        c.topic = talk.chat_line(topic, 0, ctx.days).map(|_| topic);
        // whatever's most on their mind comes first
        let pick = c.pick_for(7, 97);
        let opener = c.aware.iter().find_map(|a| a.lines(kid, pick).0);
        let v = voice(c.temper, kid);
        let hello = if first {
            c.rng.pick(v.hi).replace("{name}", name)
        } else if visits >= 3 {
            c.rng.pick(v.regular).to_string()
        } else {
            c.rng.pick(v.again).to_string()
        };
        c.line = match opener {
            Some(o) if first => format!("{hello} {o}"),
            Some(o) => o,
            // now and then something about what's going on, too
            None if c.rng.chance(0.45) => format!("{hello} {}", talk.remark(ctx, x, kid)),
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
            pos: None,
            temper: Temper::Warm,
            host: None,
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
        let first = visits == 0;
        let welcome: &[&str] = if h.church {
            &["welcome, friend. sit a while, if you like", "peace be with you. it's warmer in here, isn't it?"]
        } else {
            match temper {
                Temper::Warm => &["come in, come in! let me take your coat, dear", "you're frozen! sit by the fire, I'll put the kettle on"],
                Temper::Chatty => &["oh, a visitor! come in! excuse the mess — well, it's always like this", "come in! you have to see the tree, we did it ourselves!"],
                Temper::Shy => &["oh — hello. please, come in… make yourself at home", "h-hi. um. welcome. would you like to sit?"],
                Temper::Cheeky => &["ah, you've found our hideout! wipe your boots, mind", "welcome, welcome! no snowballs indoors. that's the rule. mostly"],
                Temper::Dreamy => &["hello… come in out of the snow. listen to the fire", "oh, how lovely. a visitor on a snowy night"],
                Temper::Gruff => &["hmph. come in, then. shut the door, you're letting the warm out", "well, don't stand there. come in. sit"],
            }
        };
        let hello = if first { format!("{} I'm {name}.", c.rng.pick(welcome)) } else { c.rng.pick(welcome).to_string() };
        c.line = if kid { format!("hi!! this is MY house! I'm {name}!") } else { hello };
        c.host = Some(h);
        c.opts = c.options(Node::Hello, ctx, talk, false);
        c
    }

    fn pick_for(&self, salt: u32, n: usize) -> usize {
        (hash(self.seed ^ salt) as usize) % n
    }

    /// The things that can be said back after `node`.
    fn options(&mut self, node: Node, ctx: &TalkCtx, talk: &Talk, cocoa: bool) -> Vec<Opt> {
        let o = |label: &str, act: Act| Opt { label: label.to_string(), act, reply: None, tag: None };
        let pick = self.pick_for(7, 97);
        // what's going on around them: the first unasked line about each
        let kid = self.kid;
        let aware_opts = |asked: &[u8], aware: &[Aware], only: Option<usize>| -> Vec<Opt> {
            let mut v = vec![];
            for (i, a) in aware.iter().enumerate().take(3) {
                if only.is_some_and(|k| k != i) {
                    continue;
                }
                let (_, lines) = a.lines(kid, pick);
                for (j, (label, reply, act)) in lines.into_iter().enumerate() {
                    let tag = (100 + i * 10 + j) as u8;
                    if asked.contains(&tag) {
                        continue;
                    }
                    let act = act.unwrap_or(Act::Go(Node::Aware(i, j)));
                    v.push(Opt { label, act, reply: Some(reply), tag: Some(tag) });
                    if only.is_none() {
                        break;
                    }
                }
            }
            v
        };
        let bye = if self.kid { o("see you later!", Act::Go(Node::Bye)) } else { o("goodbye!", Act::Go(Node::Bye)) };
        let topic = self.topic.and_then(|t| talk.chat_line(t, 0, ctx.days).map(|(mine, _)| (t, mine)));
        match node {
            Node::Hello | Node::Again if self.host.is_some() => {
                let h = self.host.clone().unwrap_or_else(|| unreachable!());
                let mut v = vec![];
                for (tag, label, act) in [
                    (21u8, if h.church { "it's beautiful in here" } else { "your home is lovely" }, Act::Go(Node::HomeLovely)),
                    (22, "tell me about your family", Act::Go(Node::Family)),
                    (23, if h.church { "how long have you looked after the church?" } else { "how long have you lived here?" }, Act::Go(Node::Years)),
                    (24, "something smells wonderful…", Act::Go(Node::Cooking)),
                    (25, "can I help with anything?", Act::Go(Node::HelpAsk)),
                    (26, "who's this little one?", Act::Go(Node::PetTalk)),
                    (27, "mind if I sit down?", Act::Sit),
                    (28, "would you play us something?", Act::PlayTune),
                ] {
                    let ok = match tag {
                        22 => !h.family.is_empty() && !self.kid,
                        24 => h.oven,
                        26 => h.pet.is_some(),
                        28 => h.piano,
                        _ => true,
                    };
                    if ok && !self.asked.contains(&tag) {
                        v.push(Opt { label: label.to_string(), act, reply: None, tag: Some(tag) });
                    }
                }
                v.truncate(4);
                v.push(bye);
                v
            }
            Node::HomeLovely | Node::Family | Node::Years | Node::PetTalk => vec![o(if self.kid { "cool!" } else { "that's lovely" }, Act::Go(Node::Again)), bye],
            Node::Cooking => vec![Opt { label: "could I try one?".into(), act: Act::Cookie, reply: Some(["of course! take two — no, take three", "go on, before they're all gone!", "here — still warm. careful!"][self.pick_for(9, 3)].into()), tag: Some(29) }, o("smells amazing", Act::Go(Node::Again)), bye],
            Node::HelpAsk => {
                let h = self.host.as_ref();
                match h.and_then(|h| h.task) {
                    Some(task) => vec![
                        Opt { label: "leave it to me!".into(), act: Act::Task(task), reply: Some(["you're a treasure", "oh, thank you!", "bless you, dear"][self.pick_for(10, 3)].into()), tag: Some(30) },
                        o("maybe later", Act::Go(Node::Again)),
                        bye,
                    ],
                    None => vec![o("alright, just say the word", Act::Go(Node::Again)), bye],
                }
            }
            Node::Hello | Node::Again => {
                let mut v = vec![];
                if self.kid {
                    for (tag, label, act) in [
                        (1, "what are you up to?", Act::Go(Node::Play)),
                        (2, "what do you want for Christmas?", Act::Go(Node::Wish)),
                        (3, "do you believe in Santa?", Act::Go(Node::Santa)),
                        (5, "snowball fight?", Act::Snowball),
                    ] {
                        if !self.asked.contains(&tag) {
                            v.push(o(label, act));
                        }
                    }
                } else {
                    for (tag, label, act) in [
                        (1, "how are you doing?", Act::Go(Node::How)),
                        (2, "what do you do around here?", Act::Go(Node::Job)),
                        (3, "any tips for a newcomer?", Act::Go(Node::Tip)),
                        (5, "walk with me a while?", Act::Invite),
                    ] {
                        if !self.asked.contains(&tag) {
                            v.push(o(label, act));
                        }
                    }
                }
                if let (Some((t, mine)), false) = (topic, self.asked.contains(&4)) {
                    v.insert(1.min(v.len()), o(&mine, Act::Go(Node::Topic(t, 0))));
                }
                // things they've noticed go first
                let mut aw = aware_opts(&self.asked, &self.aware, None);
                aw.truncate(2);
                aw.extend(v);
                let mut v = aw;
                v.truncate(4);
                v.push(bye);
                v
            }
            Node::How => vec![o("glad to hear it!", Act::Go(Node::Again)), o("anything I can do to help?", Act::Go(Node::Help)), bye],
            Node::Help => {
                let mut v = vec![];
                if cocoa {
                    v.push(o("here — have my cocoa", Act::GiveCocoa));
                }
                v.push(o("I'll keep an eye out", Act::Go(Node::Again)));
                v.push(bye);
                v
            }
            Node::Job => vec![o("tell me more!", Act::Go(Node::JobMore)), o("how long have you done that?", Act::Go(Node::JobYears)), o("something else…", Act::Go(Node::Again))],
            Node::JobMore | Node::JobYears => vec![o("ha! I'll remember that", Act::Go(Node::Again)), bye],
            Node::Tip => vec![o("thanks!", Act::Go(Node::Again)), o("got another?", Act::Go(Node::Tip)), bye],
            Node::Topic(t, s) => {
                let mut v = vec![];
                if let Some((mine, _)) = talk.chat_line(t, s + 1, ctx.days) {
                    v.push(o(&mine, Act::Go(Node::Topic(t, s + 1))));
                }
                v.push(o(if self.kid { "cool!" } else { "anything else new?" }, Act::Go(Node::Again)));
                v.push(bye);
                v
            }
            Node::Play => vec![o("can I help?", Act::Go(Node::PlayHelp)), o("sounds fun!", Act::Go(Node::Again)), bye],
            Node::PlayHelp => vec![o("deal!", Act::Go(Node::Again)), bye],
            Node::Wish => vec![o("have you been good this year?", Act::Go(Node::WishGood)), o("I hope you get it!", Act::Go(Node::WishHope)), bye],
            Node::WishGood | Node::WishHope => vec![o("ha! good luck", Act::Go(Node::Again)), bye],
            Node::Santa => vec![o("what will you leave out for him?", Act::Go(Node::SantaLeave)), o("will you stay up to see him?", Act::Go(Node::SantaStay)), bye],
            Node::SantaLeave | Node::SantaStay => vec![o("he'll love that", Act::Go(Node::Again)), bye],
            Node::Aware(i, _) => {
                // more to say about the same thing, then back to anything else
                let mut v = aware_opts(&self.asked, &self.aware, Some(i));
                v.truncate(3);
                v.push(o(if self.kid { "cool! what else?" } else { "anyway…" }, Act::Go(Node::Again)));
                v.push(bye);
                v
            }
            Node::Bye => vec![],
        }
    }

    /// What they say on reaching `node`.
    fn say(&mut self, node: Node, ctx: &TalkCtx, talk: &Talk) -> String {
        let (job, what, more) = JOBS[self.pick_for(1, JOBS.len())];
        match node {
            Node::Hello => String::new(),
            Node::Again => self.rng.pick(voice(self.temper, self.kid).more).to_string(),
            Node::How => {
                let m = voice(self.temper, self.kid).mood;
                m[self.pick_for(2, m.len())].to_string()
            }
            Node::Help => {
                if self.rng.chance(0.6) {
                    "a cocoa would go down a treat right now".into()
                } else {
                    "just keep spreading the cheer — that's help enough".into()
                }
            }
            Node::Job => format!("I'm the village {job} — {what}"),
            Node::JobMore => more.to_string(),
            Node::JobYears => format!("{} years, near enough. wouldn't trade it", 3 + self.pick_for(3, 38)),
            Node::Tip => {
                let k = self.rng.int(0, TIPS.len() as i32) as usize;
                TIPS[k].to_string()
            }
            Node::Topic(t, s) => talk.chat_line(t, s, ctx.days).map(|(_, theirs)| theirs).unwrap_or_else(|| "hm, I've lost my train of thought".into()),
            Node::Play => format!("I'm {}!", PLAY[self.pick_for(4, PLAY.len())]),
            Node::PlayHelp => PLAY_HELP[self.pick_for(5, PLAY_HELP.len())].to_string(),
            Node::Wish => format!("{}!", WISHES[self.pick_for(6, WISHES.len())]),
            Node::WishGood => self.rng.pick(GOOD).to_string(),
            Node::WishHope => "me too!! I'm keeping my fingers crossed".into(),
            Node::Santa => {
                if ctx.days > 0 && ctx.days < 40 {
                    format!("of COURSE! only {} more sleeps!", ctx.days)
                } else {
                    "of course! I heard his sleigh bells once".into()
                }
            }
            Node::SantaLeave => "cookies for him and carrots for the reindeer!".into(),
            Node::SantaStay => "I'm gonna try! I always fall asleep though".into(),
            Node::Aware(..) => "mm-hm!".into(),
            Node::HomeLovely => {
                let h = self.host.as_ref();
                let theme = h.map(|h| h.theme);
                use super::interior::Theme as T;
                let pride = match theme {
                    _ if h.is_some_and(|h| h.church) => "three hundred years of candles and carols. the stones are warm with it",
                    Some(T::Baker) => "it always smells of cinnamon. I've stopped noticing; visitors never do",
                    Some(T::Toymaker) => "mind the tin soldiers. they get everywhere. half of them are \"in testing\"",
                    Some(T::Bookworm) => "every shelf's a different winter. the one by the fire is all ghost stories",
                    Some(T::Musician) => "the piano's older than me. it's out of tune in exactly the right places",
                    Some(T::Sailor) => "every map on the wall is somewhere I've been. the telescope still finds the North Star",
                    Some(T::Candles) => "I make every candle in here. forty-two, at the last count. forty-three",
                    Some(T::Farmer) => "my grandfather built that rocking chair. it still creaks his name",
                    None => "thank you — it's not much, but it's warm",
                };
                pride.to_string()
            }
            Node::Family => {
                let fam = self.host.as_ref().map(|h| h.family.clone()).unwrap_or_default();
                match fam.as_slice() {
                    [a] => format!("that's {a} — the best thing that ever happened to this house"),
                    [a, b] => format!("{a} and {b}. noisy, wonderful, always hungry"),
                    _ => "it's just me these days. well — me, and the whole village".into(),
                }
            }
            Node::Years => {
                let y = self.host.as_ref().map(|h| h.years).unwrap_or(10);
                let memory = ["the winter the river froze, we skated to the bakery", "the first Christmas here, the chimney caught a bird's nest. it lives in the shed now", "every year the tree gets a bit bigger, and so do the kids", "I still remember the first snow. I cried, a bit. happy crying"][self.pick_for(11, 4)];
                format!("{y} years this winter. {memory}")
            }
            Node::Cooking => ["gingerbread! my mother's recipe, with a pinch of black pepper", "mince pies — the first batch always goes wrong, the second's for guests", "spiced biscuits. they're for Santa. officially"][self.pick_for(12, 3)].into(),
            Node::PetTalk => match self.host.as_ref().and_then(|h| h.pet.clone()) {
                Some((k, n)) => match k {
                    super::pets::PetKind::Dog => format!("that's {n}! he greets everyone like they've been at sea for a year"),
                    super::pets::PetKind::Cat => format!("that's {n}. she owns the house; we just keep it warm for her"),
                    super::pets::PetKind::Bunny => format!("that's {n}! she does a little twirl when she's happy"),
                },
                None => "just us, I'm afraid".into(),
            },
            Node::HelpAsk => {
                use super::interior::Task as K;
                let pet = self.host.as_ref().and_then(|h| h.pet.as_ref().map(|p| p.1.clone())).unwrap_or_else(|| "the cat".into());
                match self.host.as_ref().and_then(|h| h.task) {
                    Some(K::Star) => "oh, would you? the star keeps slipping off the top of the tree".into(),
                    Some(K::Log) => "the fire's getting low — could you put another log on?".into(),
                    Some(K::FindPet) => format!("{pet}'s hiding somewhere again. could you find them?"),
                    Some(K::Tune) => "the piano's lonely — would you play it something?".into(),
                    Some(K::Stocking) => "there's one stocking left to hang on the mantel…".into(),
                    None => "oh, you're sweet. everything's done — just enjoy yourself".into(),
                }
            }
            Node::Bye => self.rng.pick(voice(self.temper, self.kid).bye).to_string(),
        }
    }

    /// Pick option `n` (1-based). Returns (what the player said, what they
    /// said back, the act) — or None if there's no such option.
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
            Act::Invite => {
                self.asked.push(5);
                Node::Bye
            }
            Act::Snowball => {
                self.asked.push(5);
                Node::Bye
            }
            Act::GiveCocoa | Act::Cookie | Act::PlayTune => Node::Again,
            Act::Sit => Node::Again,
            Act::Task(_) => Node::Bye,
            Act::Pet(_) => Node::Bye,
            // after helping, there may be more to say about it
            Act::HelpBuild => match otag {
                Some(tag) => Node::Aware(((tag - 100) / 10) as usize, 0),
                None => Node::Again,
            },
        };
        // remember which branches have been taken
        let tag = match node {
            Node::How | Node::Play => Some(1),
            Node::Job | Node::Wish => Some(2),
            Node::Tip | Node::Santa => Some(3),
            Node::Topic(..) => Some(4),
            _ => None,
        };
        if let Some(tag) = tag {
            if !self.asked.contains(&tag) {
                self.asked.push(tag);
            }
        }
        let theirs = match (reply, act) {
            (Some(r), _) => r,
            (None, Act::Invite) => "I'd love to! lead the way".to_string(),
            (None, Act::Snowball) => "you're ON! ready… THREE!".to_string(),
            (None, Act::GiveCocoa) => "oh, how kind! thank you!".to_string(),
            (None, Act::HelpBuild) => "thanks!!".to_string(),
            (None, Act::Sit) => ["please! make yourself at home", "of course — the comfy one's free", "sit, sit! you're a guest"][self.pick_for(13, 3)].to_string(),
            (None, Act::PlayTune) => ["oh, go on then. *cracks knuckles*", "just one! …or two", "this one's my favourite"][self.pick_for(14, 3)].to_string(),
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

    pub fn draw(&self, f: &mut Frame, hover: Option<usize>) {
        let (w, h) = (f.w as i32, f.h as i32);
        let (x0, y0, bw, bh, said) = self.layout(w, h);
        let bg = Bg::Mix(pal::UI_PANEL, 0.94);
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
            f.text(x0 + 2, y, &format!("{}", i + 1), Rgb::hex(0xffd36b), bg, 0.0);
            let label: String = o.label.chars().take((bw - 8) as usize).collect();
            f.text(x0 + 5, y, &label, fg, bg, 0.0);
        }
        if !self.opts.is_empty() {
            let foot = " 1-5 or click to answer · drag to move · Esc to leave ";
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

    /// Walk every path a little way: options are never empty until goodbye,
    /// nothing's left unfilled, and goodbye ends it.
    #[test]
    fn conversations_branch_and_end() {
        let ctx = TalkCtx { days: 9, train: true, ..Default::default() };
        for kid in [false, true] {
            for seed in 0..30u32 {
                let mut talk = Talk::new(seed);
                let aware = [vec![], vec![Aware::Building { igloo: seed % 3 == 0 }, Aware::SawFeed("bunny")], vec![Aware::GotHit, Aware::Fighting], vec![Aware::WasCarried, Aware::SawDropIn, Aware::Built { igloo: false }]][seed as usize % 4].clone();
                let mut c = Convo::open(seed as u64 + 1, "Martha", kid, seed % 4, &ctx, &mut talk, 0.0, seed, aware);
                assert!(!c.line.is_empty());
                let mut rng = Rng::new(seed as u64);
                for step in 0..12 {
                    if c.ended_at.is_some() {
                        break;
                    }
                    assert!(!c.opts.is_empty() && c.opts.len() <= 5, "{:?}", c.opts.len());
                    // mostly not goodbye, so the tree gets explored
                    let n = if step < 10 { rng.int(1, c.opts.len().max(2) as i32) as usize } else { c.opts.len() };
                    let (mine, theirs, _) = c.choose(n, step as f32, &ctx, &talk, true).expect("valid option");
                    assert!(!mine.is_empty() && !theirs.is_empty());
                    assert!(!theirs.contains('{') && !mine.contains('{'), "unfilled: {mine} / {theirs}");
                }
                let n = c.opts.len();
                if n > 0 {
                    c.choose(n, 20.0, &ctx, &talk, false);
                }
                assert!(c.ended_at.is_some(), "goodbye didn't end it");
            }
        }
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
