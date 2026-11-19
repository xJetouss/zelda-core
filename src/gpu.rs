//! HD-Skalierung auf der Grafikkarte (OpenGL-Shader in einem unsichtbaren Kontext, siehe `offgl`).
//!
//! Hochgeladen wird nur das kleine Originalbild (z. B. 256×224). Pass 1 berechnet daraus eine
//! kantengeglättete Version in ungefähr Bildschirmauflösung (xBR-Regeln mit stufenloser Abdeckung,
//! also beliebiger Faktor), Pass 2 zeichnet sie ins Fenster. Die CPU hat damit fast nichts mehr zu tun.

use crate::gfx;
use glow::{self, HasContext};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// 0 = Pixel (scharf, kantengeglättet skaliert), 1 = weich (bilinear), 2 = HD weich, 3 = Clean HD
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pixel,
    Smooth,
    HdSoft,
    HdClean,
    /// MMPX 2× + 2×
    Ultra,
    /// ScaleFX 3× + rAA (umgekehrtes Antialiasing), dann weich aufs Fenster
    Max,
    /// Super-xBR 2× + 2×, dann weich aufs Fenster
    Super,
}

/// Super-xBR: Programme (Pass 0, 1, 2) und Zwischenbilder (1×, 2×, 2×, 2×, 4×, 4×)
struct Sxbr {
    p: [glow::Program; 3],
    t: [(glow::Framebuffer, glow::Texture, (i32, i32)); 6],
}

/// ScaleFX + rAA: Programme und Zwischenbilder
struct Sfx {
    p: [glow::Program; 6],
    /// Pass 0–3 in Originalgröße (Gleitkomma), danach zwei 3×-Bilder (hin und her)
    t: [(glow::Framebuffer, glow::Texture, (i32, i32)); 6],
}

struct Res {
    up: glow::Program,
    mmpx: glow::Program,
    /// MMPX-Zwischenbilder: 2× und 4×
    mx: [(glow::Framebuffer, glow::Texture, (i32, i32)); 2],
    out: glow::Program,
    vao: glow::VertexArray,
    src: glow::Texture,
    src_size: (i32, i32),
    uploaded: u64,
    fbo: glow::Framebuffer,
    fbo_tex: glow::Texture,
    fbo_size: (i32, i32),
    rendered: (u64, i32, u8),
    sfx: Option<Sfx>,
    sxbr: Option<Sxbr>,
}

struct Inner {
    rgba: Vec<u8>,
    w: i32,
    h: i32,
    version: u64,
    mode: Mode,
    res: Option<Res>,
    failed: bool,
    /// unsichtbarer OpenGL-Kontext
    ctx: Option<crate::offgl::OffGl>,
    /// Ausgabe in Fenstergröße (Bild, Größe)
    out: Option<(glow::Framebuffer, glow::Texture, (i32, i32))>,
    /// gespiegelte Kopie der Ausgabe (OpenGL zählt Zeilen von unten), daraus wird direkt gelesen
    flip: Option<(glow::Framebuffer, glow::Texture)>,
}

const VERT: &str = r#"
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = vec2(p.x, 1.0 - p.y);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// Pass 1: xBR mit stufenloser Kantenabdeckung in Zielauflösung
const FRAG_UP: &str = r#"
in vec2 v_uv;
out vec4 o_color;
uniform sampler2D u_tex;
uniform vec2 u_size;
uniform float u_w;      // Breite des Übergangs in Quellpixeln
uniform int u_soft;
vec3 fet(ivec2 p) {
    p = clamp(p, ivec2(0), ivec2(u_size) - 1);
    return texelFetch(u_tex, p, 0).rgb;
}
float df(vec3 a, vec3 b) {
    vec3 d = (a - b) * 255.0;
    float y = abs(0.299 * d.r + 0.587 * d.g + 0.114 * d.b);
    float u = abs(-0.169 * d.r - 0.331 * d.g + 0.5 * d.b);
    float v = abs(0.5 * d.r - 0.419 * d.g - 0.081 * d.b);
    return 48.0 * y + 7.0 * u + 6.0 * v;
}
bool eq(vec3 a, vec3 b) { return df(a, b) < 155.0; }
bool same(vec3 a, vec3 b) { return all(lessThan(abs(a - b), vec3(0.002))); }
ivec2 rot(ivec2 o, int k) {
    for (int i = 0; i < k; i++) { o = ivec2(-o.y, o.x); }
    return o;
}
vec2 rotinv(vec2 c, int k) {
    for (int i = 0; i < k; i++) { c = vec2(c.y, -c.x); }
    return c;
}
float cover(float x, float t) { return clamp((x - t) / u_w + 0.5, 0.0, 1.0); }
void main() {
    // Zeile 0 der Textur = oberste Bildzeile (wie beim Hochladen)
    vec2 p = vec2(v_uv.x, 1.0 - v_uv.y) * u_size;
    ivec2 ip = ivec2(floor(p));
    vec2 fp = fract(p);
    vec3 E = fet(ip);
    vec3 res = E;
    for (int k = 0; k < 4; k++) {
        vec2 lc = rotinv(fp - 0.5, k) + 0.5;
        vec3 pi = fet(ip + rot(ivec2(1, 1), k));
        vec3 ph = fet(ip + rot(ivec2(0, 1), k));
        vec3 pf = fet(ip + rot(ivec2(1, 0), k));
        if (same(E, ph) || same(E, pf)) continue;
        vec3 pg = fet(ip + rot(ivec2(-1, 1), k));
        vec3 pc = fet(ip + rot(ivec2(1, -1), k));
        vec3 pd = fet(ip + rot(ivec2(-1, 0), k));
        vec3 pb = fet(ip + rot(ivec2(0, -1), k));
        vec3 h5 = fet(ip + rot(ivec2(0, 2), k));
        vec3 i5 = fet(ip + rot(ivec2(1, 2), k));
        vec3 f4 = fet(ip + rot(ivec2(2, 0), k));
        vec3 i4 = fet(ip + rot(ivec2(2, 1), k));
        float ev = df(E, pc) + df(E, pg) + df(pi, h5) + df(pi, f4) + 4.0 * df(ph, pf);
        float iv = df(ph, pd) + df(ph, i5) + df(pf, i4) + df(pf, pb) + 4.0 * df(E, pi);
        vec3 px = df(E, pf) <= df(E, ph) ? pf : ph;
        bool cond = (!eq(pf, pb) && !eq(ph, pd)) || (eq(E, pi) && !eq(pf, i4) && !eq(ph, i5)) || eq(E, pg) || eq(E, pc);
        if (ev < iv && cond) {
            float ke = df(pf, pg);
            float ki = df(ph, pc);
            bool ex2 = !same(E, pc) && !same(pb, pc);
            bool ex3 = !same(E, pg) && !same(pd, pg);
            bool left = 2.0 * ke <= ki && ex3;
            bool up = ke >= 2.0 * ki && ex2;
            float c45 = cover(lc.x + lc.y, 1.5);
            float cl = cover(2.0 * lc.y + lc.x, 2.0);
            float cu = cover(2.0 * lc.x + lc.y, 2.0);
            float c = (left && up) ? max(cl, cu) : left ? cl : up ? cu : c45;
            res = mix(res, px, c);
        } else if (ev <= iv && u_soft == 1) {
            res = mix(res, px, 0.5 * cover(lc.x + lc.y, 1.75));
        }
    }
    o_color = vec4(res, 1.0);
}
"#;

/// Pass 2 / direkte Ausgabe: 0 = scharfe Pixel mit Kantenglättung, 1 = bilinear
const FRAG_OUT: &str = r#"
in vec2 v_uv;
out vec4 o_color;
uniform sampler2D u_tex;
uniform vec2 u_size;
uniform vec2 u_scale; // Bildschirmpixel je Texel
uniform int u_mode;
void main() {
    vec2 p = v_uv * u_size;
    vec2 uv;
    if (u_mode == 0) {
        // „sharp bilinear“: nur an den Texelgrenzen 1 Bildschirmpixel weich
        // scharfe Pixel: Texelmitte flach, nur ein Bildschirmpixel Übergang an den Kanten
        vec2 sc = max(u_scale, vec2(1.0));
        vec2 cd = fract(p) - 0.5;
        vec2 rr = 0.5 - 0.5 / sc;
        vec2 f = (cd - clamp(cd, -rr, rr)) * sc + 0.5;
        uv = (floor(p) + f) / u_size;
    } else {
        uv = v_uv;
    }
    o_color = vec4(texture(u_tex, uv).rgb, 1.0);
}
"#;

/// MMPX (McGuire & Gagiu 2021): Pixel-Art 2× vergrößern, Formen und Linien bleiben erhalten.
/// Ausgabe ist doppelt so groß wie `u_tex`; jedes Fragment berechnet sein Viertel des Quellpixels.
const FRAG_MMPX: &str = r#"
in vec2 v_uv;
out vec4 o_color;
uniform sampler2D u_tex;
uniform vec2 u_size; // Quellgröße
ivec3 px(ivec2 p) {
    p = clamp(p, ivec2(0), ivec2(u_size) - 1);
    return ivec3(round(texelFetch(u_tex, p, 0).rgb * 255.0));
}
int luma(ivec3 c) { return c.r + c.g + c.b; }
bool eq(ivec3 a, ivec3 b) { return a == b; }
bool all2(ivec3 b, ivec3 a0, ivec3 a1) { return a0 == b && a1 == b; }
bool all3(ivec3 b, ivec3 a0, ivec3 a1, ivec3 a2) { return a0 == b && a1 == b && a2 == b; }
bool all4(ivec3 b, ivec3 a0, ivec3 a1, ivec3 a2, ivec3 a3) { return a0 == b && a1 == b && a2 == b && a3 == b; }
bool any3(ivec3 b, ivec3 a0, ivec3 a1, ivec3 a2) { return a0 == b || a1 == b || a2 == b; }
bool none2(ivec3 b, ivec3 a0, ivec3 a1) { return a0 != b && a1 != b; }
bool none4(ivec3 b, ivec3 a0, ivec3 a1, ivec3 a2, ivec3 a3) { return a0 != b && a1 != b && a2 != b && a3 != b; }
void main() {
    vec2 q = vec2(v_uv.x, 1.0 - v_uv.y) * u_size * 2.0;
    ivec2 o = ivec2(floor(q));
    ivec2 p = o / 2;
    int sub = (o.x & 1) + 2 * (o.y & 1); // 0 J, 1 K, 2 L, 3 M
    int x = p.x, y = p.y;
    ivec3 A = px(ivec2(x - 1, y - 1)), B = px(ivec2(x, y - 1)), C = px(ivec2(x + 1, y - 1));
    ivec3 D = px(ivec2(x - 1, y)), E = px(ivec2(x, y)), F = px(ivec2(x + 1, y));
    ivec3 G = px(ivec2(x - 1, y + 1)), H = px(ivec2(x, y + 1)), I = px(ivec2(x + 1, y + 1));
    ivec3 J = E, K = E, L = E, M = E;
    if (A != E || B != E || C != E || D != E || F != E || G != E || H != E || I != E) {
        ivec3 P = px(ivec2(x, y - 2)), S = px(ivec2(x, y + 2)), Q = px(ivec2(x - 2, y)), R = px(ivec2(x + 2, y));
        int Bl = luma(B), Dl = luma(D), El = luma(E), Fl = luma(F), Hl = luma(H);
        // 1:1-Schrägen
        if ((D == B && D != H && D != F) && (El >= Dl || E == A) && any3(E, A, C, G) && ((El < Dl) || A != D || E != P || E != Q)) J = D;
        if ((B == F && B != D && B != H) && (El >= Bl || E == C) && any3(E, A, C, I) && ((El < Bl) || C != B || E != P || E != R)) K = B;
        if ((H == D && H != F && H != B) && (El >= Hl || E == G) && any3(E, A, G, I) && ((El < Hl) || G != H || E != S || E != Q)) L = H;
        if ((F == H && F != B && F != D) && (El >= Fl || E == I) && any3(E, C, G, I) && ((El < Fl) || I != H || E != R || E != S)) M = F;
        // Kreuzungen
        if ((E != F && all4(E, C, I, D, Q) && all2(F, B, H)) && (F != px(ivec2(x + 3, y)))) { K = F; M = F; }
        if ((E != D && all4(E, A, G, F, R) && all2(D, B, H)) && (D != px(ivec2(x - 3, y)))) { J = D; L = D; }
        if ((E != H && all4(E, G, I, B, P) && all2(H, D, F)) && (H != px(ivec2(x, y + 3)))) { L = H; M = H; }
        if ((E != B && all4(E, A, C, H, S) && all2(B, D, F)) && (B != px(ivec2(x, y - 3)))) { J = B; K = B; }
        // Dreiecksspitzen
        if (Bl < El && all4(E, G, H, I, S) && none4(E, A, D, C, F)) { J = B; K = B; }
        if (Hl < El && all4(E, A, B, C, P) && none4(E, D, G, I, F)) { L = H; M = H; }
        if (Fl < El && all4(E, A, D, G, Q) && none4(E, B, C, I, H)) { K = F; M = F; }
        if (Dl < El && all4(E, C, F, I, R) && none4(E, B, A, G, H)) { J = D; L = D; }
        // 2:1-Schrägen
        if (H != B) {
            if (H != A && H != E && H != C) {
                if (all3(H, G, F, R) && none2(H, D, px(ivec2(x + 2, y - 1)))) L = M;
                if (all3(H, I, D, Q) && none2(H, F, px(ivec2(x - 2, y - 1)))) M = L;
            }
            if (B != I && B != G && B != E) {
                if (all3(B, A, F, R) && none2(B, D, px(ivec2(x + 2, y + 1)))) J = K;
                if (all3(B, C, D, Q) && none2(B, F, px(ivec2(x - 2, y + 1)))) K = J;
            }
        }
        if (F != D) {
            if (D != I && D != E && D != C) {
                if (all3(D, A, H, S) && none2(D, B, px(ivec2(x + 1, y + 2)))) J = L;
                if (all3(D, G, B, P) && none2(D, H, px(ivec2(x + 1, y - 2)))) L = J;
            }
            if (F != E && F != A && F != G) {
                if (all3(F, C, H, S) && none2(F, B, px(ivec2(x - 1, y + 2)))) K = M;
                if (all3(F, I, B, P) && none2(F, H, px(ivec2(x - 1, y - 2)))) M = K;
            }
        }
    }
    ivec3 res = sub == 0 ? J : sub == 1 ? K : sub == 2 ? L : M;
    o_color = vec4(vec3(res) / 255.0, 1.0);
}
"#;

unsafe fn compile(gl: &glow::Context, header: &str, vs: &str, fs: &str) -> Result<glow::Program, String> {
    let prog = gl.create_program()?;
    let mut shaders = Vec::new();
    for (ty, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
        let s = gl.create_shader(ty)?;
        gl.shader_source(s, &format!("{header}{src}"));
        gl.compile_shader(s);
        if !gl.get_shader_compile_status(s) {
            return Err(gl.get_shader_info_log(s));
        }
        gl.attach_shader(prog, s);
        shaders.push(s);
    }
    gl.link_program(prog);
    if !gl.get_program_link_status(prog) {
        return Err(gl.get_program_info_log(prog));
    }
    for s in shaders {
        gl.detach_shader(prog, s);
        gl.delete_shader(s);
    }
    Ok(prog)
}

unsafe fn new_tex(gl: &glow::Context, filter: u32) -> Result<glow::Texture, String> {
    let t = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(t));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    Ok(t)
}

/// Auftrag an den HD-Thread
struct Job {
    rgba: Vec<u8>,
    w: i32,
    h: i32,
    version: u64,
    mode: Mode,
    vw: i32,
    vh: i32,
}

/// Ergebnis des HD-Threads: Schlüssel, Bild (None = fehlgeschlagen), Shader dauerhaft unbrauchbar
type Done = ((u64, u8, i32, i32), Option<gpui::RenderImage>, bool);

/// HD-Thread: besitzt den OpenGL-Kontext und alle Shader-Ressourcen
fn worker(jobs: std::sync::mpsc::Receiver<Job>, done: std::sync::mpsc::Sender<Done>) {
    let mut i = Inner { rgba: Vec::new(), w: 0, h: 0, version: 0, mode: Mode::Pixel, res: None, failed: false, ctx: None, out: None, flip: None };
    while let Ok(job) = jobs.recv() {
        let key = (job.version, job.mode as u8, job.vw, job.vh);
        if i.ctx.is_none() && !i.failed {
            match crate::offgl::OffGl::new() {
                Ok(c) => i.ctx = Some(c),
                Err(e) => {
                    eprintln!("OpenGL: {e}");
                    i.failed = true;
                }
            }
        }
        let img = if i.failed {
            None
        } else {
            if job.version != i.version {
                i.rgba = job.rgba;
                i.w = job.w;
                i.h = job.h;
                i.version = job.version;
            }
            i.mode = job.mode;
            let ctx = i.ctx.take().unwrap();
            ctx.make_current();
            let img = unsafe { render_gl(&mut i, &ctx.gl, job.vw, job.vh) };
            i.ctx = Some(ctx);
            img
        };
        if done.send((key, img, i.failed)).is_err() {
            break;
        }
    }
}

/// Bildseite (UI-Thread): sammelt das kleine Bild, schickt Aufträge, hält das zuletzt fertige Bild
struct Front {
    rgba: Vec<u8>,
    w: i32,
    h: i32,
    version: u64,
    failed: bool,
    /// zuletzt ausgegebenes Bild und sein Schlüssel (Bildnummer, Modus, Größe)
    last: Option<((u64, u8, i32, i32), Arc<gpui::RenderImage>)>,
    /// Auftrag läuft gerade (Schlüssel)
    busy: Option<(u64, u8, i32, i32)>,
    /// Bild darf ein Anzeige-Bild hinterherhinken (hohe Bildwiederholrate)
    pipelined: bool,
    chan: Option<(std::sync::mpsc::Sender<Job>, std::sync::mpsc::Receiver<Done>)>,
}

#[derive(Clone)]
pub struct GpuScreen {
    inner: Rc<RefCell<Front>>,
}

impl GpuScreen {
    pub fn new() -> Self {
        GpuScreen { inner: Rc::new(RefCell::new(Front { rgba: Vec::new(), w: 0, h: 0, version: 0, failed: false, last: None, busy: None, pipelined: false, chan: None })) }
    }

    /// Shader nicht verfügbar (dann zeichnet die App wie früher über die CPU).
    pub fn failed(&self) -> bool {
        self.inner.borrow().failed
    }

    /// Bei hoher Bildwiederholrate darf das HD-Bild im Hintergrund entstehen und ein Anzeige-Bild später
    /// erscheinen – dann bleibt im UI-Thread genug Zeit für jedes Anzeige-Bild.
    pub fn set_pipelined(&self, on: bool) {
        self.inner.borrow_mut().pipelined = on;
    }

    /// Neues Bild (RGBA-Farben) übergeben.
    pub fn set_frame(&self, px: &[gfx::Color32], w: usize, h: usize) {
        let mut i = self.inner.borrow_mut();
        i.rgba.clear();
        i.rgba.reserve(px.len() * 4);
        for c in px {
            i.rgba.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
        }
        i.w = w as i32;
        i.h = h as i32;
        i.version += 1;
    }

    /// fertiges Ergebnis übernehmen
    fn accept(i: &mut Front, d: Done) {
        let (key, img, failed) = d;
        i.busy = None;
        i.failed |= failed;
        if let Some(img) = img {
            crate::perf::new_image();
            if let Some((_, old)) = i.last.replace((key, Arc::new(img))) {
                gfx::retire_image(old);
            }
        }
    }

    /// auf das Ergebnis des laufenden Auftrags warten
    fn wait(i: &mut Front) {
        match i.chan.as_ref().and_then(|(_, r)| r.recv().ok()) {
            Some(d) => Self::accept(i, d),
            None => i.failed = true,
        }
    }

    /// Bild in `vw`×`vh` Bildschirmpixeln berechnen (nur wenn sich etwas geändert hat)
    pub fn render(&self, mode: Mode, vw: i32, vh: i32) -> Option<Arc<gpui::RenderImage>> {
        let mut guard = self.inner.borrow_mut();
        let i = &mut *guard;
        if i.failed || i.w == 0 || vw <= 0 || vh <= 0 {
            return None;
        }
        if i.chan.is_none() {
            let (jt, jr) = std::sync::mpsc::channel::<Job>();
            let (dt, dr) = std::sync::mpsc::channel::<Done>();
            if std::thread::Builder::new().name("hd".into()).spawn(move || worker(jr, dt)).is_err() {
                i.failed = true;
                return None;
            }
            i.chan = Some((jt, dr));
        }
        // fertige Ergebnisse abholen (ohne zu warten)
        while let Some(d) = i.chan.as_ref().and_then(|(_, r)| r.try_recv().ok()) {
            Self::accept(i, d);
        }
        if i.failed {
            return None;
        }
        let key = (i.version, mode as u8, vw, vh);
        if i.last.as_ref().is_some_and(|(k, _)| *k == key) {
            return i.last.as_ref().map(|(_, img)| img.clone());
        }
        if i.busy != Some(key) {
            if i.busy.is_some() {
                // noch ein älterer Auftrag unterwegs: erst abwarten (der Thread arbeitet der Reihe nach)
                Self::wait(i);
                if i.failed {
                    return None;
                }
            }
            let job = Job { rgba: i.rgba.clone(), w: i.w, h: i.h, version: i.version, mode, vw, vh };
            if i.chan.as_ref().is_none_or(|(t, _)| t.send(job).is_err()) {
                i.failed = true;
                return None;
            }
            i.busy = Some(key);
        }
        // im Hintergrund weiterrechnen lassen, solange ein passend großes Bild da ist; sonst warten
        let same_size = i.last.as_ref().is_some_and(|((_, m, w, h), _)| *m == mode as u8 && *w == vw && *h == vh);
        if !(i.pipelined && same_size) {
            Self::wait(i);
        }
        if i.failed {
            return None;
        }
        i.last.as_ref().map(|(_, img)| img.clone())
    }
}

/// Ausgabe-Bild anlegen, Filter rendern, Ergebnis als BGRA zurücklesen
unsafe fn render_gl(i: &mut Inner, gl: &glow::Context, vw: i32, vh: i32) -> Option<gpui::RenderImage> {
    let need = i.out.map_or(true, |o| o.2 != (vw, vh));
    if need {
        if let Some((f, t, _)) = i.out.take() {
            gl.delete_framebuffer(f);
            gl.delete_texture(t);
        }
        if let Some((f, t)) = i.flip.take() {
            gl.delete_framebuffer(f);
            gl.delete_texture(t);
        }
        let t = new_tex(gl, glow::NEAREST).ok()?;
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, vw, vh, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
        let f = gl.create_framebuffer().ok()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(t), 0);
        i.flip = Some((f, t));
        let t = new_tex(gl, glow::NEAREST).ok()?;
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, vw, vh, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
        let f = gl.create_framebuffer().ok()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(t), 0);
        i.out = Some((f, t, (vw, vh)));
    }
    let (fbo, _, _) = i.out.unwrap();
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.viewport(0, 0, vw, vh);
    draw(i, gl, fbo, vw, vh);
    if i.failed {
        return None;
    }
    // auf der Grafikkarte senkrecht spiegeln, dann ohne Umkopieren direkt ins Ausgabebild lesen
    let (flip, _) = i.flip?;
    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(flip));
    gl.blit_framebuffer(0, 0, vw, vh, 0, vh, vw, 0, glow::COLOR_BUFFER_BIT, glow::NEAREST);
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(flip));
    let mut out = vec![0u8; (vw * vh * 4) as usize];
    gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
    gl.read_pixels(0, 0, vw, vh, glow::BGRA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut out[..]));
    gfx::render_image(vw as u32, vh as u32, out)
}

struct Vp {
    width_px: i32,
    height_px: i32,
}

unsafe fn draw(i: &mut Inner, gl: &glow::Context, out_fbo: glow::Framebuffer, vw: i32, vh: i32) {
    if i.failed || i.w == 0 {
        return;
    }
    if i.res.is_none() {
        let header = "#version 140
".to_string();
        let r = (|| -> Result<Res, String> {
            let up = compile(gl, &header, VERT, FRAG_UP)?;
            let mmpx = compile(gl, &header, VERT, FRAG_MMPX)?;
            let mx = [(gl.create_framebuffer()?, new_tex(gl, glow::NEAREST)?, (0, 0)), (gl.create_framebuffer()?, new_tex(gl, glow::LINEAR)?, (0, 0))];
            let out = compile(gl, &header, VERT, FRAG_OUT)?;
            let vao = gl.create_vertex_array()?;
            let src = new_tex(gl, glow::NEAREST)?;
            let fbo_tex = new_tex(gl, glow::LINEAR)?;
            let fbo = gl.create_framebuffer()?;
            // ScaleFX: wenn es nicht übersetzt, bleibt „HD Max“ bei MMPX
            let sfx = (|| -> Result<Sfx, String> {
                use crate::gpu_sfx as x;
                let p = [compile(gl, &header, VERT, &x::pass0())?, compile(gl, &header, VERT, &x::pass1())?, compile(gl, &header, VERT, &x::pass2())?, compile(gl, &header, VERT, &x::pass3())?, compile(gl, &header, VERT, &x::pass4())?, compile(gl, &header, VERT, &x::raa())?];
                let mut t = Vec::new();
                for k in 0..6 {
                    t.push((gl.create_framebuffer()?, new_tex(gl, if k == 4 { glow::LINEAR } else { glow::NEAREST })?, (0, 0)));
                }
                Ok(Sfx { p, t: [t[0], t[1], t[2], t[3], t[4], t[5]] })
            })();
            if let Err(e) = &sfx {
                eprintln!("ScaleFX-Shader: {e}");
            }
            let sxbr = (|| -> Result<Sxbr, String> {
                use crate::gpu_sxbr as x;
                let p = [compile(gl, &header, VERT, &x::pass0())?, compile(gl, &header, VERT, &x::pass1())?, compile(gl, &header, VERT, &x::pass2())?];
                let mut t = Vec::new();
                for k in 0..6 {
                    t.push((gl.create_framebuffer()?, new_tex(gl, if k == 5 { glow::LINEAR } else { glow::NEAREST })?, (0, 0)));
                }
                Ok(Sxbr { p, t: [t[0], t[1], t[2], t[3], t[4], t[5]] })
            })();
            if let Err(e) = &sxbr {
                eprintln!("Super-xBR-Shader: {e}");
            }
            Ok(Res { up, mmpx, mx, out, vao, src, src_size: (0, 0), uploaded: 0, fbo, fbo_tex, fbo_size: (0, 0), rendered: (0, 0, 9), sfx: sfx.ok(), sxbr: sxbr.ok() })
        })();
        match r {
            Ok(r) => i.res = Some(r),
            Err(e) => {
                eprintln!("GPU-Shader: {e}");
                i.failed = true;
                return;
            }
        }
    }
    let (w, h, version, mode) = (i.w, i.h, i.version, i.mode);
    let Inner { rgba, res, .. } = i;
    let r = res.as_mut().unwrap();
    gl.active_texture(glow::TEXTURE0);
    gl.bind_texture(glow::TEXTURE_2D, Some(r.src));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
    if r.src_size != (w, h) {
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(rgba.as_slice()));
        r.src_size = (w, h);
        r.uploaded = version;
    } else if r.uploaded != version {
        gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(rgba.as_slice()));
        r.uploaded = version;
    }
    let vp = Vp { width_px: vw, height_px: vh };
    gl.disable(glow::BLEND);
    gl.bind_vertex_array(Some(r.vao));
    let set2 = |prog: glow::Program, name: &str, x: f32, y: f32| {
        let l = gl.get_uniform_location(prog, name);
        gl.uniform_2_f32(l.as_ref(), x, y);
    };
    let out_tex;
    let out_size;
    let mode = if (mode == Mode::Max && r.sfx.is_none()) || (mode == Mode::Super && r.sxbr.is_none()) { Mode::Ultra } else { mode };
    match mode {
        Mode::Super => {
            let key = (version, 4, mode as u8);
            let sx = r.sxbr.as_mut().unwrap();
            if r.rendered != key {
                gl.disable(glow::SCISSOR_TEST);
                let sizes = [(w, h), (w * 2, h * 2), (w * 2, h * 2), (w * 2, h * 2), (w * 4, h * 4), (w * 4, h * 4)];
                for k in 0..6 {
                    let (fbo, tex, size) = &mut sx.t[k];
                    if *size != sizes[k] {
                        gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
                        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, sizes[k].0, sizes[k].1, 0, glow::RGBA, glow::FLOAT, None);
                        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(*fbo));
                        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(*tex), 0);
                        *size = sizes[k];
                    }
                }
                let t = |k: usize| sx.t[k].1;
                // (Programm, Ziel, Quelle, Quellgröße, Original für Pass 1)
                let steps: [(usize, usize, glow::Texture, (i32, i32), Option<glow::Texture>); 6] = [
                    (0, 0, r.src, (w, h), None),
                    (1, 1, t(0), (w, h), Some(r.src)),
                    (2, 2, t(1), (w * 2, h * 2), None),
                    (0, 3, t(2), (w * 2, h * 2), None),
                    (1, 4, t(3), (w * 2, h * 2), Some(t(2))),
                    (2, 5, t(4), (w * 4, h * 4), None),
                ];
                for (pi, ti, src_tex, ssize, orig) in steps {
                    let prog = sx.p[pi];
                    let (fbo, _, osize) = sx.t[ti];
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.viewport(0, 0, osize.0, osize.1);
                    gl.use_program(Some(prog));
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(src_tex));
                    gl.uniform_1_i32(gl.get_uniform_location(prog, "u_tex").as_ref(), 0);
                    if let Some(o) = orig {
                        gl.active_texture(glow::TEXTURE1);
                        gl.bind_texture(glow::TEXTURE_2D, Some(o));
                        gl.uniform_1_i32(gl.get_uniform_location(prog, "u_orig").as_ref(), 1);
                        gl.active_texture(glow::TEXTURE0);
                    }
                    gl.uniform_2_f32(gl.get_uniform_location(prog, "u_size").as_ref(), ssize.0 as f32, ssize.1 as f32);
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(out_fbo));
                gl.viewport(0, 0, vp.width_px, vp.height_px);
                r.rendered = key;
            }
            out_tex = sx.t[5].1;
            out_size = sx.t[5].2;
        }
        Mode::Max => {
            let key = (version, 3, mode as u8);
            let sfx = r.sfx.as_mut().unwrap();
            if r.rendered != key {
                gl.disable(glow::SCISSOR_TEST);
                let (w3, h3) = (w * 3, h * 3);
                // Ziel anlegen/anpassen: 0–3 Gleitkomma in Originalgröße, 4/5 = 3×
                for k in 0..6 {
                    let (fbo, tex, size) = &mut sfx.t[k];
                    let want = if k < 4 { (w, h) } else { (w3, h3) };
                    if *size != want {
                        gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
                        if k < 4 {
                            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, want.0, want.1, 0, glow::RGBA, glow::FLOAT, None);
                        } else {
                            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, want.0, want.1, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
                        }
                        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(*fbo));
                        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(*tex), 0);
                        *size = want;
                    }
                }
                // (Programm, Ziel, Quelle u_tex, Quellgröße, zweite Quelle (Name, Textur), Richtung für rAA)
                let t = |k: usize| sfx.t[k].1;
                let steps: [(usize, usize, glow::Texture, (i32, i32), Option<(&str, glow::Texture)>, (i32, i32)); 7] = [
                    (0, 0, r.src, (w, h), None, (0, 0)),
                    (1, 1, t(0), (w, h), None, (0, 0)),
                    (2, 2, t(1), (w, h), Some(("u_m", t(0))), (0, 0)),
                    (3, 3, t(2), (w, h), None, (0, 0)),
                    (4, 4, t(3), (w, h), Some(("u_orig", r.src)), (0, 0)),
                    (5, 5, t(4), (w3, h3), None, (1, 0)),
                    (5, 4, t(5), (w3, h3), None, (0, 1)),
                ];
                // saubere Pixel in höherer Auflösung: ohne rAA-Nachglätten (nur ScaleFX)
                for &(pi, ti, src_tex, ssize, extra, dir) in &steps[..5] {
                    let prog = sfx.p[pi];
                    let (fbo, _, osize) = sfx.t[ti];
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.viewport(0, 0, osize.0, osize.1);
                    gl.use_program(Some(prog));
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(src_tex));
                    gl.uniform_1_i32(gl.get_uniform_location(prog, "u_tex").as_ref(), 0);
                    if let Some((name, tex2)) = extra {
                        gl.active_texture(glow::TEXTURE1);
                        gl.bind_texture(glow::TEXTURE_2D, Some(tex2));
                        gl.uniform_1_i32(gl.get_uniform_location(prog, name).as_ref(), 1);
                        gl.active_texture(glow::TEXTURE0);
                    }
                    gl.uniform_2_f32(gl.get_uniform_location(prog, "u_size").as_ref(), ssize.0 as f32, ssize.1 as f32);
                    gl.uniform_2_i32(gl.get_uniform_location(prog, "u_dir").as_ref(), dir.0, dir.1);
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(out_fbo));
                gl.viewport(0, 0, vp.width_px, vp.height_px);
                r.rendered = key;
            }
            out_tex = sfx.t[4].1;
            out_size = sfx.t[4].2;
        }
        Mode::HdSoft | Mode::HdClean => {
            // Faktor so, dass Pass 1 etwa die Bildschirmauflösung hat
            let f = ((vp.height_px as f32 / h as f32).ceil() as i32).clamp(2, 8);
            let (fw, fh) = (w * f, h * f);
            if r.fbo_size != (fw, fh) {
                gl.bind_texture(glow::TEXTURE_2D, Some(r.fbo_tex));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, fw, fh, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(r.fbo));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(r.fbo_tex), 0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(out_fbo));
                r.fbo_size = (fw, fh);
                r.rendered = (0, 0, 9);
            }
            let key = (version, f, mode as u8);
            if r.rendered != key {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(r.fbo));
                gl.disable(glow::SCISSOR_TEST);
                gl.viewport(0, 0, fw, fh);
                gl.use_program(Some(r.up));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(r.src));
                gl.uniform_1_i32(gl.get_uniform_location(r.up, "u_tex").as_ref(), 0);
                set2(r.up, "u_size", w as f32, h as f32);
                let soft = mode == Mode::HdSoft;
                gl.uniform_1_f32(gl.get_uniform_location(r.up, "u_w").as_ref(), if soft { 0.55 } else { 1.0 / f as f32 });
                gl.uniform_1_i32(gl.get_uniform_location(r.up, "u_soft").as_ref(), soft as i32);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(out_fbo));
                gl.viewport(0, 0, vp.width_px, vp.height_px);
                r.rendered = key;
            }
            out_tex = r.fbo_tex;
            out_size = (fw, fh);
        }
        Mode::Ultra => {
            // zwei MMPX-Durchgänge: Quelle -> 2× -> 4×
            let key = (version, 4, mode as u8);
            if r.rendered != key {
                gl.disable(glow::SCISSOR_TEST);
                gl.use_program(Some(r.mmpx));
                let mut src_tex = r.src;
                let (mut sw, mut sh) = (w, h);
                for k in 0..2 {
                    let (fbo, tex, size) = &mut r.mx[k];
                    let (tw, th) = (sw * 2, sh * 2);
                    if *size != (tw, th) {
                        gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
                        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, tw, th, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
                        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(*fbo));
                        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(*tex), 0);
                        *size = (tw, th);
                    }
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(*fbo));
                    gl.viewport(0, 0, tw, th);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(src_tex));
                    gl.uniform_1_i32(gl.get_uniform_location(r.mmpx, "u_tex").as_ref(), 0);
                    let l = gl.get_uniform_location(r.mmpx, "u_size");
                    gl.uniform_2_f32(l.as_ref(), sw as f32, sh as f32);
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                    src_tex = *tex;
                    sw = tw;
                    sh = th;
                }
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(out_fbo));
                gl.viewport(0, 0, vp.width_px, vp.height_px);
                r.rendered = key;
            }
            out_tex = r.mx[1].1;
            out_size = r.mx[1].2;
        }
        _ => {
            out_tex = r.src;
            out_size = (w, h);
        }
    }
    gl.use_program(Some(r.out));
    gl.active_texture(glow::TEXTURE0);
    gl.bind_texture(glow::TEXTURE_2D, Some(out_tex));
    let lin = mode != Mode::Pixel;
    if out_tex == r.src {
        let f = if lin { glow::LINEAR } else { glow::NEAREST };
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, f as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, if mode == Mode::Pixel { glow::LINEAR } else { f } as i32);
    }
    gl.uniform_1_i32(gl.get_uniform_location(r.out, "u_tex").as_ref(), 0);
    set2(r.out, "u_size", out_size.0 as f32, out_size.1 as f32);
    set2(r.out, "u_scale", vp.width_px as f32 / out_size.0 as f32, vp.height_px as f32 / out_size.1 as f32);
    // Original und Pixel ×3: scharfe Pixel (nur an den Kanten 1 Bildschirmpixel Übergang)
    gl.uniform_1_i32(gl.get_uniform_location(r.out, "u_mode").as_ref(), if mode == Mode::Pixel || mode == Mode::Max { 0 } else { 1 });
    gl.draw_arrays(glow::TRIANGLES, 0, 3);
    if out_tex == r.src {
        // für Pass 1 wieder exakt
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    }
    gl.bind_vertex_array(None);
}
