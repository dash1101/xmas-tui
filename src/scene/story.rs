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
    Bye,
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
}

pub struct Opt {
    pub label: String,
    pub act: Act,
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

const MOODS: &[&str] = &["can't complain — the snow's perfect", "a little tired, but it's a good tired", "wonderful! I just had the best cocoa", "busy, busy, busy — it's that time of year", "cold toes, warm heart!", "all the better for a chat"];

const AGAIN: &[&str] = &["anything else on your mind?", "what else can I tell you?", "go on, I've got time", "and what about you?"];

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

const BYES: &[&str] = &["take care now!", "merry Christmas!", "see you around the village", "stay warm out there!", "lovely talking with you!"];

const KID_HI: &[&str] = &["hi!! I'm {name}!", "hiya! I'm {name}! who are you?", "oh! hi! I'm {name}"];
const KID_AGAIN: &[&str] = &["what else what else?", "ask me something else!", "okay what now?", "and then what?"];
const PLAY: &[&str] = &["building the BIGGEST snowman", "looking for reindeer tracks", "counting the lights on the big tree — I'm at 300!", "practising my snowball throw", "catching snowflakes on my tongue", "making snow angels. I've made eleven"];
const PLAY_HELP: &[&str] = &["yes!! you can be on my team", "okay, but I'm the boss", "you can hold my mittens", "only if you do the heavy bits"];
const WISHES: &[&str] = &["a sled that goes SUPER fast", "a puppy. a real one", "a telescope, to watch for Santa", "a drum kit! mum says no", "a dinosaur. a small one", "snow that never ever melts"];
const GOOD: &[&str] = &["SO good. mostly. okay — a little bit good", "I tidied my room! well, I pushed it all under the bed", "I shared my cookies. one of them", "I've been the goodest"];
const KID_BYE: &[&str] = &["bye!!", "see ya!", "BYE! merry Christmas!", "bye bye!"];

impl Convo {
    /// Start a conversation with `name` (`first`: never met before).
    pub fn open(id: u64, name: &str, kid: bool, first: bool, ctx: &TalkCtx, talk: &mut Talk, x: f32, seed: u32) -> Self {
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
            rng: Rng::new((seed ^ id as u32) as u64 ^ 0x5a1),
        };
        // something topical to talk about, if there is anything going on
        let topic = talk.chat_topic(ctx, x, kid);
        c.topic = talk.chat_line(topic, 0, ctx.days).map(|_| topic);
        c.line = if first {
            if kid {
                c.rng.pick(KID_HI).replace("{name}", name)
            } else {
                talk.intro(ctx, x, name, false)
            }
        } else {
            let hi = if kid { "oh hi again!!" } else { "oh, hello again!" };
            format!("{hi} {}", talk.remark(ctx, x, kid))
        };
        c.opts = c.options(Node::Hello, ctx, talk, false);
        c
    }

    fn pick_for(&self, salt: u32, n: usize) -> usize {
        (hash(self.seed ^ salt) as usize) % n
    }

    /// The things that can be said back after `node`.
    fn options(&mut self, node: Node, ctx: &TalkCtx, talk: &Talk, cocoa: bool) -> Vec<Opt> {
        let o = |label: &str, act: Act| Opt { label: label.to_string(), act };
        let bye = if self.kid { o("see you later!", Act::Go(Node::Bye)) } else { o("goodbye!", Act::Go(Node::Bye)) };
        let topic = self.topic.and_then(|t| talk.chat_line(t, 0, ctx.days).map(|(mine, _)| (t, mine)));
        match node {
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
            Node::Bye => vec![],
        }
    }

    /// What they say on reaching `node`.
    fn say(&mut self, node: Node, ctx: &TalkCtx, talk: &Talk) -> String {
        let (job, what, more) = JOBS[self.pick_for(1, JOBS.len())];
        match node {
            Node::Hello => String::new(),
            Node::Again => self.rng.pick(if self.kid { KID_AGAIN } else { AGAIN }).to_string(),
            Node::How => MOODS[self.pick_for(2, MOODS.len())].to_string(),
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
            Node::Bye => self.rng.pick(if self.kid { KID_BYE } else { BYES }).to_string(),
        }
    }

    /// Pick option `n` (1-based). Returns (what the player said, what they
    /// said back, the act) — or None if there's no such option.
    pub fn choose(&mut self, n: usize, t: f32, ctx: &TalkCtx, talk: &Talk, cocoa: bool) -> Option<(String, String, Act)> {
        if self.ended_at.is_some() {
            return None;
        }
        let opt = self.opts.get(n.checked_sub(1)?)?;
        let (mine, act) = (opt.label.clone(), opt.act);
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
            Act::GiveCocoa => Node::Again,
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
        let theirs = match act {
            Act::Invite => "I'd love to! lead the way".to_string(),
            Act::Snowball => "you're ON! ready… THREE!".to_string(),
            Act::GiveCocoa => "oh, how kind! thank you!".to_string(),
            Act::Go(_) => self.say(node, ctx, talk),
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
        let x0 = (w - bw) / 2;
        let y0 = (h - bh - 1).max(0);
        (x0, y0, bw, bh, said)
    }

    /// Option under (x, y) on screen, if any (1-based).
    pub fn option_at(&self, x: i32, y: i32, w: i32, h: i32) -> Option<usize> {
        let (x0, y0, bw, _, said) = self.layout(w, h);
        let first = y0 + 1 + said.len() as i32 + 1;
        let i = y - first;
        (x > x0 && x < x0 + bw - 1 && i >= 0 && (i as usize) < self.opts.len()).then_some(i as usize + 1)
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
            let foot = " 1-5 or click to answer · Esc to leave ";
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
                let mut c = Convo::open(seed as u64 + 1, "Martha", kid, seed % 2 == 0, &ctx, &mut talk, 0.0, seed);
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
            let mut c = Convo::open(42, "Henry", false, false, &ctx, talk, 0.0, 7);
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
