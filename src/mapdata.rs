//! Zusatzdaten für die Karte: Flaschen, Upgrades, Haupt- und Nebenquests.
//! Alles wird direkt aus dem Spielstand (SRAM-Spiegel $7EF000..) gelesen.

use crate::alttp::ram8;
use crate::snes::Snes;

/// Art eines Zusatz-Orts auf der Karte
#[derive(Clone, Copy, PartialEq)]
pub enum PoiKind {
    Bottle,
    Upgrade,
    Quest,
}

/// Fester Ort (Welt, Gebiet, Versatz im Gebiet) mit Prüfung, ob schon erledigt
pub struct Poi {
    #[allow(dead_code)]
    pub kind: PoiKind,
    pub de: &'static str,
    pub en: &'static str,
    /// Gebiet (0x00..0x7F) – die Position wird daraus berechnet
    pub area: u8,
    /// Versatz im Gebiet (0..1 je Achse, auf 512 Pixel)
    pub off: (f32, f32),
    /// (erreicht, Ziel)
    pub prog: fn(&Snes) -> (u32, u32),
}

impl Poi {
    pub fn pos(&self) -> (u8, f32, f32) {
        let w = (self.area >= 0x40) as u8;
        let a = self.area & 0x3F;
        ((w), (a % 8) as f32 * 512.0 + self.off.0 * 512.0, (a / 8) as f32 * 512.0 + self.off.1 * 512.0)
    }
    pub fn done(&self, s: &Snes) -> bool {
        let (a, b) = (self.prog)(s);
        a >= b
    }
}

fn r(s: &Snes, a: u32) -> u8 {
    ram8(s, a)
}
fn ge(s: &Snes, a: u32, v: u8) -> (u32, u32) {
    ((r(s, a) >= v) as u32, 1)
}
pub fn bottles(s: &Snes) -> u32 {
    (0..4).filter(|i| r(s, 0x7EF35C + i) != 0).count() as u32
}

/// Flaschen (4) – welche genau es war, merkt sich das Spiel nicht; gezählt wird die Gesamtzahl.
pub const BOTTLES: [Poi; 4] = [
    Poi { kind: PoiKind::Bottle, de: "Flasche: Händler in Kakariko (100 Rubine)", en: "Bottle: Kakariko merchant (100 rupees)", area: 0x18, off: (0.62, 0.95), prog: |s| ((bottles(s) >= 1) as u32, 1) },
    Poi { kind: PoiKind::Bottle, de: "Flasche: Truhe hinter der Kneipe", en: "Bottle: chest behind the pub", area: 0x18, off: (0.35, 1.15), prog: |s| ((bottles(s) >= 2) as u32, 1) },
    Poi { kind: PoiKind::Bottle, de: "Flasche: Landstreicher unter der Brücke", en: "Bottle: hobo under the bridge", area: 0x2D, off: (0.45, 0.55), prog: |s| ((bottles(s) >= 3) as u32, 1) },
    Poi { kind: PoiKind::Bottle, de: "Flasche: lila Truhe (Schmiede-Haus)", en: "Bottle: purple chest (smiths' house)", area: 0x22, off: (0.75, 0.75), prog: |s| ((bottles(s) >= 4) as u32, 1) },
];

/// Upgrades mit festem Ort auf der Oberwelt
pub const UPGRADES: [Poi; 11] = [
    Poi { kind: PoiKind::Upgrade, de: "Gehärtetes Schwert (Schmiede)", en: "Tempered Sword (Smithy)", area: 0x22, off: (0.35, 0.45), prog: |s| ge(s, 0x7EF359, 3) },
    Poi { kind: PoiKind::Upgrade, de: "Goldenes Schwert (Pyramiden-Fee)", en: "Golden Sword (Pyramid fairy)", area: 0x5B, off: (1.0, 1.55), prog: |s| ge(s, 0x7EF359, 4) },
    Poi { kind: PoiKind::Upgrade, de: "Silberpfeile (Pyramiden-Fee)", en: "Silver Arrows (Pyramid fairy)", area: 0x5B, off: (1.15, 1.55), prog: |s| ge(s, 0x7EF340, 3) },
    Poi { kind: PoiKind::Upgrade, de: "Feuerschild (Wunschbrunnen)", en: "Fire Shield (Waterfall of Wishing)", area: 0x0F, off: (0.3, 0.55), prog: |s| ge(s, 0x7EF35A, 2) },
    Poi { kind: PoiKind::Upgrade, de: "Magischer Bumerang (Wunschbrunnen)", en: "Magic Boomerang (Waterfall of Wishing)", area: 0x0F, off: (0.45, 0.55), prog: |s| ge(s, 0x7EF341, 2) },
    Poi { kind: PoiKind::Upgrade, de: "Halbe Magie (Fledermaus)", en: "Half Magic (Mad Batter)", area: 0x22, off: (0.6, 0.2), prog: |s| ge(s, 0x7EF37B, 1) },
    Poi { kind: PoiKind::Upgrade, de: "Magisches Puder (Hexe)", en: "Magic Powder (Witch)", area: 0x16, off: (0.4, 0.45), prog: |s| ge(s, 0x7EF344, 2) },
    Poi { kind: PoiKind::Upgrade, de: "Flöte (Spukhain)", en: "Flute (Haunted Grove)", area: 0x2A, off: (0.5, 0.5), prog: |s| ge(s, 0x7EF34C, 2) },
    Poi { kind: PoiKind::Upgrade, de: "Flöte aktivieren (Wetterhahn)", en: "Activate flute (weathervane)", area: 0x18, off: (0.95, 0.7), prog: |s| ge(s, 0x7EF34C, 3) },
    Poi { kind: PoiKind::Upgrade, de: "Mehr Bomben (Glücksteich)", en: "More bombs (Pond of Happiness)", area: 0x35, off: (1.05, 0.25), prog: |s| (r(s, 0x7EF370).min(7) as u32, 7) },
    Poi { kind: PoiKind::Upgrade, de: "Mehr Pfeile (Glücksteich)", en: "More arrows (Pond of Happiness)", area: 0x35, off: (1.2, 0.25), prog: |s| (r(s, 0x7EF371).min(7) as u32, 7) },
];

/// Sprite-ID der Figur, an der ein Ort tatsächlich liegt (0 = keine; dann geschätzte Position).
/// IDs aus dem zelda3-Projekt (kSpriteActiveRoutines).
pub const BOTTLE_NPC: [u8; 4] = [0x75, 0, 0x2B, 0x39];
pub const UPGRADE_NPC: [u8; 11] = [0x1A, 0xC8, 0xC8, 0x72, 0x72, 0x3A, 0x36, 0x2E, 0, 0x72, 0x72];
/// je Quest in `QUESTS` (Hauptquests 0)
pub const QUEST_NPC: [u8; 33] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // Hauptquest
    0x75, 0x1F, 0xAD, 0x16, 0x52, 0, 0x36, 0x2E, 0, 0x1A, 0xB4, 0x3A, 0xC0, 0xF2, 0xF2, 0x72, 0x72, 0x2B, 0xC8,
];

/// Position einer Figur: dieselbe Welt, am nächsten zur geschätzten Stelle (sonst die Schätzung)
pub fn npc_pos(npcs: &[(u8, u8, f32, f32)], id: u8, est: (u8, f32, f32)) -> (u8, f32, f32) {
    if id == 0 {
        return est;
    }
    npcs.iter()
        .filter(|n| n.0 == id && n.1 == est.0)
        .map(|n| (n, (n.2 - est.1).powi(2) + (n.3 - est.2).powi(2)))
        .filter(|(_, d)| *d < 800.0 * 800.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| (n.1, n.2, n.3))
        .unwrap_or(est)
}

/// Freiwillige Upgrades (zählen nicht für 100 %): halbe Magie, mehr Bomben, mehr Pfeile
pub const OPTIONAL_UPGRADES: [usize; 3] = [5, 9, 10];

/// Ungefähr benötigte Gegenstände je Dungeon (Index wie `tracker::DUNGEONS`): (Adresse, Mindestwert, Deutsch, Englisch).
/// Adresse 0 = alle 7 Kristalle.
pub const DUNGEON_REQS: [&[(u32, u8, &str, &str)]; 13] = [
    &[(0x7EF34A, 1, "Laterne", "Lamp")],
    &[(0x7EF34A, 1, "Laterne", "Lamp"), (0x7EF340, 1, "Bogen", "Bow")],
    &[(0x7EF34E, 1, "Buch Mudora", "Book of Mudora"), (0x7EF354, 1, "Krafthandschuh", "Power Glove"), (0x7EF34A, 1, "Laterne", "Lamp")],
    &[(0x7EF354, 1, "Krafthandschuh", "Power Glove"), (0x7EF34A, 1, "Laterne", "Lamp"), (0x7EF353, 1, "Zauberspiegel", "Magic Mirror")],
    &[(0x7EF357, 1, "Mondperle", "Moon Pearl"), (0x7EF34B, 1, "Hammer", "Hammer"), (0x7EF340, 1, "Bogen", "Bow"), (0x7EF34A, 1, "Laterne", "Lamp")],
    &[(0x7EF357, 1, "Mondperle", "Moon Pearl"), (0x7EF353, 1, "Zauberspiegel", "Magic Mirror"), (0x7EF356, 1, "Zora-Flossen", "Flippers"), (0x7EF342, 1, "Enterhaken", "Hookshot"), (0x7EF34B, 1, "Hammer", "Hammer")],
    &[(0x7EF357, 1, "Mondperle", "Moon Pearl"), (0x7EF345, 1, "Feuerstab", "Fire Rod")],
    &[(0x7EF357, 1, "Mondperle", "Moon Pearl"), (0x7EF354, 1, "Krafthandschuh", "Power Glove"), (0x7EF34B, 1, "Hammer", "Hammer")],
    &[(0x7EF357, 1, "Mondperle", "Moon Pearl"), (0x7EF354, 2, "Titanhandschuh", "Titan's Mitt"), (0x7EF356, 1, "Zora-Flossen", "Flippers"), (0x7EF345, 1, "Feuerstab", "Fire Rod"), (0x7EF34B, 1, "Hammer", "Hammer")],
    &[(0x7EF357, 1, "Mondperle", "Moon Pearl"), (0x7EF354, 2, "Titanhandschuh", "Titan's Mitt"), (0x7EF34C, 3, "Flöte", "Flute"), (0x7EF348, 1, "Äther", "Ether"), (0x7EF350, 1, "Somaria-Stab", "Cane of Somaria"), (0x7EF345, 1, "Feuerstab", "Fire Rod")],
    &[(0x7EF357, 1, "Mondperle", "Moon Pearl"), (0x7EF354, 2, "Titanhandschuh", "Titan's Mitt"), (0x7EF34B, 1, "Hammer", "Hammer"), (0x7EF349, 1, "Quake", "Quake"), (0x7EF350, 1, "Somaria-Stab", "Cane of Somaria"), (0x7EF345, 1, "Feuerstab", "Fire Rod"), (0x7EF346, 1, "Eisstab", "Ice Rod")],
    &[(0, 7, "Alle 7 Kristalle", "All 7 crystals"), (0x7EF342, 1, "Enterhaken", "Hookshot"), (0x7EF355, 1, "Pegasusstiefel", "Pegasus Boots"), (0x7EF34B, 1, "Hammer", "Hammer"), (0x7EF340, 1, "Bogen", "Bow"), (0x7EF345, 1, "Feuerstab", "Fire Rod"), (0x7EF350, 1, "Somaria-Stab", "Cane of Somaria")],
    &[(0x7EF340, 3, "Silberpfeile", "Silver Arrows"), (0x7EF359, 2, "Master-Schwert", "Master Sword"), (0x7EF345, 1, "Feuerstab", "Fire Rod")],
];

/// Hat Link den Gegenstand?
pub fn req_ok(s: &Snes, addr: u32, min: u8) -> bool {
    if addr == 0 {
        r(s, 0x7EF37A).count_ones() as u8 >= min
    } else {
        r(s, addr) >= min
    }
}

/// Upgrades aus Dungeons (nur in der Liste): (Name, Dungeon-Index in DUNGEONS, Prüfung)
pub const DUNGEON_UPGRADES: [(&str, &str, usize, fn(&Snes) -> bool); 5] = [
    ("Krafthandschuh", "Power Glove", 2, |s| r(s, 0x7EF354) >= 1),
    ("Titanhandschuh", "Titan's Mitt", 7, |s| r(s, 0x7EF354) >= 2),
    ("Blaues Gewand", "Blue Mail", 8, |s| r(s, 0x7EF35B) >= 1),
    ("Rotes Gewand", "Red Mail", 11, |s| r(s, 0x7EF35B) >= 2),
    ("Spiegelschild", "Mirror Shield", 10, |s| r(s, 0x7EF35A) >= 3),
];

/// Kristall-Bits in $7EF37A je Dungeon (PoD, Sumpf, Skelett, Diebe, Eis, Moor, Schildkröte)
pub const CRYSTAL_BITS: [(usize, u8); 7] = [(4, 0x02), (5, 0x10), (6, 0x40), (7, 0x20), (8, 0x04), (9, 0x01), (10, 0x08)];

#[derive(Clone, Copy, PartialEq)]
pub enum QState {
    Done,
    Active,
    Locked,
}

pub struct Quest {
    pub main: bool,
    pub de: &'static str,
    pub en: &'static str,
    pub hint_de: &'static str,
    pub hint_en: &'static str,
    /// Startpunkt auf der Karte (Gebiet, Versatz) – nur in der Spoiler-Ansicht
    pub start: Option<(u8, (f32, f32))>,
    pub done: fn(&Snes) -> bool,
    pub avail: fn(&Snes) -> bool,
}

impl Quest {
    pub fn state(&self, s: &Snes) -> QState {
        if (self.done)(s) {
            QState::Done
        } else if (self.avail)(s) {
            QState::Active
        } else {
            QState::Locked
        }
    }
    pub fn pos(&self) -> Option<(u8, f32, f32)> {
        self.start.map(|(a, o)| Poi { kind: PoiKind::Quest, de: "", en: "", area: a, off: o, prog: |_| (0, 1) }.pos())
    }
}

fn pend(s: &Snes, b: u8) -> bool {
    r(s, 0x7EF374) & b != 0
}
fn cry(s: &Snes, b: u8) -> bool {
    r(s, 0x7EF37A) & b != 0
}
fn has(s: &Snes, a: u32) -> bool {
    r(s, a) != 0
}
fn stage(s: &Snes) -> u8 {
    r(s, 0x7EF3C5)
}
fn ms(s: &Snes) -> bool {
    r(s, 0x7EF359) >= 2
}

macro_rules! q {
    ($m:expr, $de:expr, $en:expr, $hde:expr, $hen:expr, $start:expr, $done:expr, $avail:expr) => {
        Quest { main: $m, de: $de, en: $en, hint_de: $hde, hint_en: $hen, start: $start, done: $done, avail: $avail }
    };
}

pub const QUESTS: &[Quest] = &[
    // ---- Hauptquest
    q!(true, "Prinzessin Zelda retten", "Rescue Princess Zelda", "Im Kerker von Hyrule-Schloss", "In the Hyrule Castle dungeon", None, |s| stage(s) >= 2, |_| true),
    q!(true, "Anhänger des Mutes", "Pendant of Courage", "Ostpalast", "Eastern Palace", None, |s| pend(s, 0x04), |s| stage(s) >= 2),
    q!(true, "Anhänger der Kraft", "Pendant of Power", "Wüstenpalast – Buch Mudora nötig", "Desert Palace – needs the Book of Mudora", None, |s| pend(s, 0x02), |s| stage(s) >= 2),
    q!(true, "Anhänger der Weisheit", "Pendant of Wisdom", "Turm des Herakles – Spiegel nötig", "Tower of Hera – needs the mirror", None, |s| pend(s, 0x01), |s| stage(s) >= 2),
    q!(true, "Master-Schwert holen", "Get the Master Sword", "Verlorene Wälder – alle 3 Anhänger", "Lost Woods – all 3 pendants", None, |s| ms(s), |s| r(s, 0x7EF374) & 7 == 7),
    q!(true, "Agahnim besiegen", "Defeat Agahnim", "Schlossturm", "Castle Tower", None, |s| stage(s) >= 3, |s| ms(s)),
    q!(true, "Kristall: Palast der Finsternis", "Crystal: Palace of Darkness", "Schattenwelt Ost", "Dark World east", None, |s| cry(s, 0x02), |s| stage(s) >= 3),
    q!(true, "Kristall: Sumpfpalast", "Crystal: Swamp Palace", "Damm in der Lichtwelt öffnen", "Open the dam in the Light World", None, |s| cry(s, 0x10), |s| stage(s) >= 3 && has(s, 0x7EF356)),
    q!(true, "Kristall: Skelettwald", "Crystal: Skull Woods", "Schattenwelt Nordwest", "Dark World northwest", None, |s| cry(s, 0x40), |s| stage(s) >= 3),
    q!(true, "Kristall: Diebesversteck", "Crystal: Thieves' Town", "Dorf der Verlorenen", "Village of Outcasts", None, |s| cry(s, 0x20), |s| stage(s) >= 3 && has(s, 0x7EF354)),
    q!(true, "Kristall: Eispalast", "Crystal: Ice Palace", "Titanhandschuh + Flossen", "Titan's Mitt + flippers", None, |s| cry(s, 0x04), |s| r(s, 0x7EF354) >= 2 && has(s, 0x7EF356)),
    q!(true, "Kristall: Moorpalast", "Crystal: Misery Mire", "Flöte + Äther-Medaillon", "Flute + Ether medallion", None, |s| cry(s, 0x01), |s| r(s, 0x7EF34C) >= 3 && r(s, 0x7EF354) >= 2),
    q!(true, "Kristall: Schildkrötenfels", "Crystal: Turtle Rock", "Somaria-Stab + Quake", "Cane of Somaria + Quake", None, |s| cry(s, 0x08), |s| has(s, 0x7EF350) && r(s, 0x7EF354) >= 2),
    q!(true, "Ganons Turm und Ganon", "Ganon's Tower and Ganon", "Alle 7 Kristalle", "All 7 crystals", None, |_| false, |s| r(s, 0x7EF37A) & 0x7F == 0x7F),
    // ---- Nebenquests
    q!(false, "Flaschenhändler", "Bottle merchant", "Kakariko, 100 Rubine", "Kakariko, 100 rupees", Some((0x18, (0.62, 0.95))), |s| bottles(s) >= 1, |s| stage(s) >= 2),
    q!(false, "Krankes Kind", "Sick kid", "Mit einer Flasche besuchen – Käfernetz", "Visit with a bottle – Bug Net", Some((0x18, (0.3, 0.75))), |s| has(s, 0x7EF34D), |s| bottles(s) >= 1),
    q!(false, "Verirrter alter Mann", "Lost old man", "Todesberg, Laterne nötig – Spiegel", "Death Mountain, needs the lamp – Mirror", Some((0x03, (0.55, 1.6))), |s| has(s, 0x7EF353), |s| has(s, 0x7EF354) && has(s, 0x7EF34A)),
    q!(false, "Sahasrahla", "Sahasrahla", "Mit dem Anhänger des Mutes – Pegasusstiefel", "With the Pendant of Courage – Pegasus Boots", Some((0x1E, (0.15, 0.55))), |s| has(s, 0x7EF355), |s| pend(s, 0x04)),
    q!(false, "König Zora", "King Zora", "500 Rubine – Zora-Flossen", "500 rupees – Zora's Flippers", Some((0x0F, (0.8, 0.25))), |s| has(s, 0x7EF356), |s| has(s, 0x7EF354)),
    q!(false, "Bibliothek", "Library", "Pegasusstiefel – Buch Mudora", "Pegasus Boots – Book of Mudora", Some((0x29, (0.5, 0.5))), |s| has(s, 0x7EF34E), |s| has(s, 0x7EF355)),
    q!(false, "Pilz zur Hexe bringen", "Bring the mushroom to the witch", "Magisches Puder", "Magic Powder", Some((0x16, (0.4, 0.45))), |s| r(s, 0x7EF344) >= 2, |s| r(s, 0x7EF344) >= 1),
    q!(false, "Flöte im Spukhain", "Flute in the Haunted Grove", "Mit der Schaufel graben", "Dig with the shovel", Some((0x2A, (0.5, 0.5))), |s| r(s, 0x7EF34C) >= 2, |s| r(s, 0x7EF34C) >= 1),
    q!(false, "Flöte aktivieren", "Activate the flute", "Wetterhahn in Kakariko", "Weathervane in Kakariko", Some((0x18, (0.95, 0.7))), |s| r(s, 0x7EF34C) >= 3, |s| r(s, 0x7EF34C) >= 2),
    q!(false, "Schmiede wieder vereinen", "Reunite the smiths", "Frosch im Dorf der Verlorenen – Gehärtetes Schwert", "Frog in the Village of Outcasts – Tempered Sword", Some((0x58, (1.2, 1.4))), |s| r(s, 0x7EF359) >= 3, |s| ms(s) && r(s, 0x7EF354) >= 2 && has(s, 0x7EF357)),
    q!(false, "Lila Truhe", "Purple chest", "Nach der Schmiede – Flasche", "After the smiths – Bottle", Some((0x62, (0.75, 0.75))), |s| bottles(s) >= 4, |s| r(s, 0x7EF359) >= 3),
    q!(false, "Verrückte Fledermaus", "Mad Batter", "Hammer + Puder – Halbe Magie", "Hammer + powder – Half Magic", Some((0x22, (0.6, 0.2))), |s| r(s, 0x7EF37B) >= 1, |s| has(s, 0x7EF34B) && r(s, 0x7EF344) >= 2),
    q!(false, "Katzenfisch", "Catfish", "Stein in den Teich werfen – Quake", "Throw a rock into the pond – Quake", Some((0x4F, (0.5, 0.5))), |s| has(s, 0x7EF349), |s| has(s, 0x7EF357) && has(s, 0x7EF354)),
    q!(false, "Äther-Tafel", "Ether tablet", "Buch + Master-Schwert", "Book + Master Sword", Some((0x03, (0.1, 0.12))), |s| has(s, 0x7EF348), |s| has(s, 0x7EF34E) && ms(s)),
    q!(false, "Bombos-Tafel", "Bombos tablet", "Buch + Spiegel + Master-Schwert", "Book + mirror + Master Sword", Some((0x30, (1.1, 1.85))), |s| has(s, 0x7EF347), |s| has(s, 0x7EF34E) && has(s, 0x7EF353) && ms(s)),
    q!(false, "Wunschbrunnen", "Waterfall of Wishing", "Flossen – Feuerschild & Bumerang", "Flippers – Fire Shield & boomerang", Some((0x0F, (0.35, 0.55))), |s| r(s, 0x7EF35A) >= 2 && r(s, 0x7EF341) >= 2, |s| has(s, 0x7EF356)),
    q!(false, "Glücksteich", "Pond of Happiness", "Rubine werfen – mehr Bomben/Pfeile", "Throw rupees – more bombs/arrows", Some((0x35, (1.1, 0.25))), |s| r(s, 0x7EF370) >= 7 && r(s, 0x7EF371) >= 7, |s| has(s, 0x7EF356)),
    q!(false, "Landstreicher", "Hobo", "Unter der Brücke (Flossen) – Flasche", "Under the bridge (flippers) – Bottle", Some((0x2D, (0.45, 0.55))), |s| bottles(s) >= 3, |s| has(s, 0x7EF356)),
    q!(false, "Pyramiden-Fee", "Pyramid fairy", "Kristall 5 & 6 – Goldenes Schwert", "Crystals 5 & 6 – Golden Sword", Some((0x5B, (1.0, 1.55))), |s| r(s, 0x7EF359) >= 4, |s| cry(s, 0x04) && cry(s, 0x01)),
];
