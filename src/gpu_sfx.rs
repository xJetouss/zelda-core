//! „HD Max“: ScaleFX (3×) + rAA-post3x, portiert aus libretro/glsl-shaders.
//!
//! ScaleFX und rAA-post3x: Copyright (c) 2016-2018 Sp00kyFox (ScaleFX@web.de), MIT-Lizenz:
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
//! associated documentation files (the "Software"), to deal in the Software without restriction,
//! including without limitation the rights to use, copy, modify, merge, publish, distribute,
//! sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions: The above copyright notice and this
//! permission notice shall be included in all copies or substantial portions of the Software.
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
//!
//! ScaleFX erkennt Kanten bis Stufe 6 und rundet sie sauber ab, ohne neue Farben zu erfinden;
//! rAA glättet danach die Treppen weich (umgekehrtes Antialiasing). Alle Durchgänge arbeiten mit
//! gl_FragCoord und texelFetch – Zeile 0 ist überall die oberste Bildzeile.

/// gemeinsamer Kopf: Quelle lesen (Rand wiederholen)
const HEAD: &str = r#"
out vec4 o_color;
uniform sampler2D u_tex;
uniform vec2 u_size;
vec4 T(ivec2 q) { return texelFetch(u_tex, clamp(q, ivec2(0), ivec2(u_size) - 1), 0); }
"#;

/// Pass 0: Farbabstände zu den Nachbarn (A B C über E, F rechts)
pub fn pass0() -> String {
    format!(
        "{HEAD}{}",
        r#"
float dist(vec3 A, vec3 B) {
    float r = 0.5 * (A.r + B.r);
    vec3 d = A - B;
    vec3 c = vec3(2. + r, 4., 3. - r);
    return sqrt(dot(c * d, d)) / 3.;
}
void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec3 A = T(p + ivec2(-1, -1)).rgb;
    vec3 B = T(p + ivec2( 0, -1)).rgb;
    vec3 C = T(p + ivec2( 1, -1)).rgb;
    vec3 E = T(p).rgb;
    vec3 F = T(p + ivec2( 1, 0)).rgb;
    o_color = vec4(dist(E, A), dist(E, B), dist(E, C), dist(E, F));
}
"#
    )
}

/// Pass 1: Eckenstärke
pub fn pass1() -> String {
    format!(
        "{HEAD}{}",
        r#"
#define SFX_CLR 0.5
#define SFX_SAA 1.0
float str(float d, vec2 a, vec2 b) {
    float diff = a.x - a.y;
    float wght1 = max(SFX_CLR - d, 0.) / SFX_CLR;
    float wght2 = clamp((1. - d) + (min(a.x, b.x) + a.x > min(a.y, b.y) + a.y ? diff : -diff), 0., 1.);
    return (SFX_SAA == 1. || 2. * d < a.x + a.y) ? (wght1 * wght2) * (a.x * a.y) : 0.;
}
void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec4 A = T(p + ivec2(-1, -1)), B = T(p + ivec2(0, -1));
    vec4 D = T(p + ivec2(-1, 0)), E = T(p), F = T(p + ivec2(1, 0));
    vec4 G = T(p + ivec2(-1, 1)), H = T(p + ivec2(0, 1)), I = T(p + ivec2(1, 1));
    vec4 res;
    res.x = str(D.z, vec2(D.w, E.y), vec2(A.w, D.y));
    res.y = str(F.x, vec2(E.w, E.y), vec2(B.w, F.y));
    res.z = str(H.z, vec2(E.w, H.y), vec2(H.w, I.y));
    res.w = str(H.x, vec2(D.w, H.y), vec2(G.w, G.y));
    o_color = res;
}
"#
    )
}

/// Pass 2: Kreuzungen auflösen (u_tex = Stärke aus Pass 1, u_m = Abstände aus Pass 0)
pub fn pass2() -> String {
    format!(
        "{HEAD}{}",
        r#"
uniform sampler2D u_m;
vec4 M(ivec2 q) { return texelFetch(u_m, clamp(q, ivec2(0), ivec2(u_size) - 1), 0); }
#define LE(x, y) (1. - step(y, x))
#define GE(x, y) (1. - step(x, y))
#define LEQ(x, y) step(x, y)
#define GEQ(x, y) step(y, x)
#define NOT(x) (1. - (x))
vec4 dom(vec3 x, vec3 y, vec3 z, vec3 w) {
    return 2. * vec4(x.y, y.y, z.y, w.y) - (vec4(x.x, y.x, z.x, w.x) + vec4(x.z, y.z, z.z, w.z));
}
float clr_(vec2 crn, vec2 a, vec2 b) {
    return (crn.x >= max(min(a.x, a.y), min(b.x, b.y))) && (crn.y >= max(min(a.x, b.y), min(b.x, a.y))) ? 1. : 0.;
}
void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec4 A = M(p + ivec2(-1, -1)), B = M(p + ivec2(0, -1));
    vec4 D = M(p + ivec2(-1, 0)), E = M(p), F = M(p + ivec2(1, 0));
    vec4 G = M(p + ivec2(-1, 1)), H = M(p + ivec2(0, 1)), I = M(p + ivec2(1, 1));
    vec4 As = T(p + ivec2(-1, -1)), Bs = T(p + ivec2(0, -1)), Cs = T(p + ivec2(1, -1));
    vec4 Ds = T(p + ivec2(-1, 0)), Es = T(p), Fs = T(p + ivec2(1, 0));
    vec4 Gs = T(p + ivec2(-1, 1)), Hs = T(p + ivec2(0, 1)), Is = T(p + ivec2(1, 1));
    vec4 jSx = vec4(As.z, Bs.w, Es.x, Ds.y), jDx = dom(As.yzw, Bs.zwx, Es.wxy, Ds.xyz);
    vec4 jSy = vec4(Bs.z, Cs.w, Fs.x, Es.y), jDy = dom(Bs.yzw, Cs.zwx, Fs.wxy, Es.xyz);
    vec4 jSz = vec4(Es.z, Fs.w, Is.x, Hs.y), jDz = dom(Es.yzw, Fs.zwx, Is.wxy, Hs.xyz);
    vec4 jSw = vec4(Ds.z, Es.w, Hs.x, Gs.y), jDw = dom(Ds.yzw, Es.zwx, Hs.wxy, Gs.xyz);
    vec4 zero4 = vec4(0.);
    vec4 jx = min(GE(jDx, zero4) * (LEQ(jDx.yzwx, zero4) * LEQ(jDx.wxyz, zero4) + GE(jDx + jDx.zwxy, jDx.yzwx + jDx.wxyz)), 1.);
    vec4 jy = min(GE(jDy, zero4) * (LEQ(jDy.yzwx, zero4) * LEQ(jDy.wxyz, zero4) + GE(jDy + jDy.zwxy, jDy.yzwx + jDy.wxyz)), 1.);
    vec4 jz = min(GE(jDz, zero4) * (LEQ(jDz.yzwx, zero4) * LEQ(jDz.wxyz, zero4) + GE(jDz + jDz.zwxy, jDz.yzwx + jDz.wxyz)), 1.);
    vec4 jw = min(GE(jDw, zero4) * (LEQ(jDw.yzwx, zero4) * LEQ(jDw.wxyz, zero4) + GE(jDw + jDw.zwxy, jDw.yzwx + jDw.wxyz)), 1.);
    vec4 res;
    res.x = min(jx.z + NOT(jx.y) * NOT(jx.w) * GE(jSx.z, 0.) * (jx.x + GE(jSx.x + jSx.z, jSx.y + jSx.w)), 1.);
    res.y = min(jy.w + NOT(jy.z) * NOT(jy.x) * GE(jSy.w, 0.) * (jy.y + GE(jSy.y + jSy.w, jSy.x + jSy.z)), 1.);
    res.z = min(jz.x + NOT(jz.w) * NOT(jz.y) * GE(jSz.x, 0.) * (jz.z + GE(jSz.x + jSz.z, jSz.y + jSz.w)), 1.);
    res.w = min(jw.y + NOT(jw.x) * NOT(jw.z) * GE(jSw.y, 0.) * (jw.w + GE(jSw.y + jSw.w, jSw.x + jSw.z)), 1.);
    res = min(res * (vec4(jx.z, jy.w, jz.x, jw.y) + NOT(res.wxyz * res.yzwx)), 1.);
    vec4 clr;
    clr.x = clr_(vec2(D.z, E.x), vec2(D.w, E.y), vec2(A.w, D.y));
    clr.y = clr_(vec2(F.x, E.z), vec2(E.w, E.y), vec2(B.w, F.y));
    clr.z = clr_(vec2(H.z, I.x), vec2(E.w, H.y), vec2(H.w, I.y));
    clr.w = clr_(vec2(H.x, G.z), vec2(D.w, H.y), vec2(G.w, G.y));
    vec4 h = vec4(min(D.w, A.w), min(E.w, B.w), min(E.w, H.w), min(D.w, G.w));
    vec4 v = vec4(min(E.y, D.y), min(E.y, F.y), min(H.y, I.y), min(H.y, G.y));
    vec4 ori = GE(h + vec4(D.w, E.w, E.w, D.w), v + vec4(E.y, E.y, H.y, H.y));
    vec4 hori = LE(h, v) * clr;
    vec4 vert = GE(h, v) * clr;
    o_color = (res + 2. * hori + 4. * vert + 8. * ori) / 15.;
}
"#
    )
}

/// Pass 3: Kantenstufen 1–6 -> Unterpixel-Auswahl
pub fn pass3() -> String {
    format!(
        "{HEAD}{}",
        r#"
#define SFX_SCN 1.0
bvec4 loadCorn(vec4 x) { return bvec4(floor(mod(x * 15. + 0.5, 2.))); }
bvec4 loadHori(vec4 x) { return bvec4(floor(mod(x * 7.5 + 0.25, 2.))); }
bvec4 loadVert(vec4 x) { return bvec4(floor(mod(x * 3.75 + 0.125, 2.))); }
bvec4 loadOr(vec4 x) { return bvec4(floor(mod(x * 1.875 + 0.0625, 2.))); }
void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec4 E = T(p);
    vec4 D = T(p + ivec2(-1, 0)), D0 = T(p + ivec2(-2, 0)), D1 = T(p + ivec2(-3, 0));
    vec4 F = T(p + ivec2(1, 0)), F0 = T(p + ivec2(2, 0)), F1 = T(p + ivec2(3, 0));
    vec4 B = T(p + ivec2(0, -1)), B0 = T(p + ivec2(0, -2)), B1 = T(p + ivec2(0, -3));
    vec4 H = T(p + ivec2(0, 1)), H0 = T(p + ivec2(0, 2)), H1 = T(p + ivec2(0, 3));
    bvec4 Ec = loadCorn(E), Eh = loadHori(E), Ev = loadVert(E), Eo = loadOr(E);
    bvec4 Dc = loadCorn(D), Dh = loadHori(D), Do = loadOr(D), D0c = loadCorn(D0), D0h = loadHori(D0), D1h = loadHori(D1);
    bvec4 Fc = loadCorn(F), Fh = loadHori(F), Fo = loadOr(F), F0c = loadCorn(F0), F0h = loadHori(F0), F1h = loadHori(F1);
    bvec4 Bc = loadCorn(B), Bv = loadVert(B), Bo = loadOr(B), B0c = loadCorn(B0), B0v = loadVert(B0), B1v = loadVert(B1);
    bvec4 Hc = loadCorn(H), Hv = loadVert(H), Ho = loadOr(H), H0c = loadCorn(H0), H0v = loadVert(H0), H1v = loadVert(H1);
    bool lvl1x = Ec.x && (Dc.z || Bc.z || SFX_SCN == 1.);
    bool lvl1y = Ec.y && (Fc.w || Bc.w || SFX_SCN == 1.);
    bool lvl1z = Ec.z && (Fc.x || Hc.x || SFX_SCN == 1.);
    bool lvl1w = Ec.w && (Dc.y || Hc.y || SFX_SCN == 1.);
    bvec2 lvl2x = bvec2((Ec.x && Eh.y) && Dc.z, (Ec.y && Eh.x) && Fc.w);
    bvec2 lvl2y = bvec2((Ec.y && Ev.z) && Bc.w, (Ec.z && Ev.y) && Hc.x);
    bvec2 lvl2z = bvec2((Ec.w && Eh.z) && Dc.y, (Ec.z && Eh.w) && Fc.x);
    bvec2 lvl2w = bvec2((Ec.x && Ev.w) && Bc.z, (Ec.w && Ev.x) && Hc.y);
    bvec2 lvl3x = bvec2(lvl2x.y && (Dh.y && Dh.x) && Fh.z, lvl2w.y && (Bv.w && Bv.x) && Hv.z);
    bvec2 lvl3y = bvec2(lvl2x.x && (Fh.x && Fh.y) && Dh.w, lvl2y.y && (Bv.z && Bv.y) && Hv.w);
    bvec2 lvl3z = bvec2(lvl2z.x && (Fh.w && Fh.z) && Dh.x, lvl2y.x && (Hv.y && Hv.z) && Bv.x);
    bvec2 lvl3w = bvec2(lvl2z.y && (Dh.z && Dh.w) && Fh.y, lvl2w.x && (Hv.x && Hv.w) && Bv.y);
    bvec2 lvl4x = bvec2((Dc.x && Dh.y && Eh.x && Eh.y && Fh.x && Fh.y) && (D0c.z && D0h.w), (Bc.x && Bv.w && Ev.x && Ev.w && Hv.x && Hv.w) && (B0c.z && B0v.y));
    bvec2 lvl4y = bvec2((Fc.y && Fh.x && Eh.y && Eh.x && Dh.y && Dh.x) && (F0c.w && F0h.z), (Bc.y && Bv.z && Ev.y && Ev.z && Hv.y && Hv.z) && (B0c.w && B0v.x));
    bvec2 lvl4z = bvec2((Fc.z && Fh.w && Eh.z && Eh.w && Dh.z && Dh.w) && (F0c.x && F0h.y), (Hc.z && Hv.y && Ev.z && Ev.y && Bv.z && Bv.y) && (H0c.x && H0v.w));
    bvec2 lvl4w = bvec2((Dc.w && Dh.z && Eh.w && Eh.z && Fh.w && Fh.z) && (D0c.y && D0h.x), (Hc.w && Hv.x && Ev.w && Ev.x && Bv.w && Bv.x) && (H0c.y && H0v.z));
    bvec2 lvl5x = bvec2(lvl4x.x && (F0h.x && F0h.y) && (D1h.z && D1h.w), lvl4y.x && (D0h.y && D0h.x) && (F1h.w && F1h.z));
    bvec2 lvl5y = bvec2(lvl4y.y && (H0v.y && H0v.z) && (B1v.w && B1v.x), lvl4z.y && (B0v.z && B0v.y) && (H1v.x && H1v.w));
    bvec2 lvl5z = bvec2(lvl4w.x && (F0h.w && F0h.z) && (D1h.y && D1h.x), lvl4z.x && (D0h.z && D0h.w) && (F1h.x && F1h.y));
    bvec2 lvl5w = bvec2(lvl4x.y && (H0v.x && H0v.w) && (B1v.z && B1v.y), lvl4w.y && (B0v.w && B0v.x) && (H1v.y && H1v.z));
    bvec2 lvl6x = bvec2(lvl5x.y && (D1h.y && D1h.x), lvl5w.y && (B1v.w && B1v.x));
    bvec2 lvl6y = bvec2(lvl5x.x && (F1h.x && F1h.y), lvl5y.y && (B1v.z && B1v.y));
    bvec2 lvl6z = bvec2(lvl5z.x && (F1h.w && F1h.z), lvl5y.x && (H1v.y && H1v.z));
    bvec2 lvl6w = bvec2(lvl5z.y && (D1h.z && D1h.w), lvl5w.x && (H1v.x && H1v.w));
    vec4 crn;
    crn.x = (lvl1x && Eo.x || lvl3x.x && Eo.y || lvl4x.x && Do.x || lvl6x.x && Fo.y) ? 5. : (lvl1x || lvl3x.y && !Eo.w || lvl4x.y && !Bo.x || lvl6x.y && !Ho.w) ? 1. : lvl3x.x ? 3. : lvl3x.y ? 7. : lvl4x.x ? 2. : lvl4x.y ? 6. : lvl6x.x ? 4. : lvl6x.y ? 8. : 0.;
    crn.y = (lvl1y && Eo.y || lvl3y.x && Eo.x || lvl4y.x && Fo.y || lvl6y.x && Do.x) ? 5. : (lvl1y || lvl3y.y && !Eo.z || lvl4y.y && !Bo.y || lvl6y.y && !Ho.z) ? 3. : lvl3y.x ? 1. : lvl3y.y ? 7. : lvl4y.x ? 4. : lvl4y.y ? 6. : lvl6y.x ? 2. : lvl6y.y ? 8. : 0.;
    crn.z = (lvl1z && Eo.z || lvl3z.x && Eo.w || lvl4z.x && Fo.z || lvl6z.x && Do.w) ? 7. : (lvl1z || lvl3z.y && !Eo.y || lvl4z.y && !Ho.z || lvl6z.y && !Bo.y) ? 3. : lvl3z.x ? 1. : lvl3z.y ? 5. : lvl4z.x ? 4. : lvl4z.y ? 8. : lvl6z.x ? 2. : lvl6z.y ? 6. : 0.;
    crn.w = (lvl1w && Eo.w || lvl3w.x && Eo.z || lvl4w.x && Do.w || lvl6w.x && Fo.z) ? 7. : (lvl1w || lvl3w.y && !Eo.x || lvl4w.y && !Ho.w || lvl6w.y && !Bo.x) ? 1. : lvl3w.x ? 3. : lvl3w.y ? 5. : lvl4w.x ? 2. : lvl4w.y ? 8. : lvl6w.x ? 4. : lvl6w.y ? 6. : 0.;
    vec4 mid;
    mid.x = (lvl2x.x &&  Eo.x || lvl2x.y &&  Eo.y || lvl5x.x &&  Do.x || lvl5x.y &&  Fo.y) ? 5. : lvl2x.x ? 1. : lvl2x.y ? 3. : lvl5x.x ? 2. : lvl5x.y ? 4. : (Ec.x && Dc.z && Ec.y && Fc.w) ? ( Eo.x ?  Eo.y ? 5. : 3. : 1.) : 0.;
    mid.y = (lvl2y.x && !Eo.y || lvl2y.y && !Eo.z || lvl5y.x && !Bo.y || lvl5y.y && !Ho.z) ? 3. : lvl2y.x ? 5. : lvl2y.y ? 7. : lvl5y.x ? 6. : lvl5y.y ? 8. : (Ec.y && Bc.w && Ec.z && Hc.x) ? (!Eo.y ? !Eo.z ? 3. : 7. : 5.) : 0.;
    mid.z = (lvl2z.x &&  Eo.w || lvl2z.y &&  Eo.z || lvl5z.x &&  Do.w || lvl5z.y &&  Fo.z) ? 7. : lvl2z.x ? 1. : lvl2z.y ? 3. : lvl5z.x ? 2. : lvl5z.y ? 4. : (Ec.z && Fc.x && Ec.w && Dc.y) ? ( Eo.z ?  Eo.w ? 7. : 1. : 3.) : 0.;
    mid.w = (lvl2w.x && !Eo.x || lvl2w.y && !Eo.w || lvl5w.x && !Bo.x || lvl5w.y && !Ho.w) ? 1. : lvl2w.x ? 5. : lvl2w.y ? 7. : lvl5w.x ? 6. : lvl5w.y ? 8. : (Ec.w && Hc.y && Ec.x && Bc.z) ? (!Eo.w ? !Eo.x ? 1. : 5. : 7.) : 0.;
    o_color = (crn + 9. * mid) / 80.;
}
"#
    )
}

/// Pass 4: 3× ausgeben – jedes Unterpixel nimmt die Farbe eines Nachbarn (u_tex = Pass 3, u_orig = Originalbild)
pub fn pass4() -> String {
    format!(
        "{HEAD}{}",
        r#"
uniform sampler2D u_orig;
vec4 loadCrn(vec4 x) { return floor(mod(x * 80. + 0.5, 9.)); }
vec4 loadMid(vec4 x) { return floor(mod(x * 8.888888 + 0.055555, 9.)); }
void main() {
    ivec2 o = ivec2(gl_FragCoord.xy);
    ivec2 s = o / 3;
    ivec2 fp = o - s * 3;
    vec4 E = T(s);
    vec4 crn = loadCrn(E);
    vec4 mid = loadMid(E);
    float sp = fp.y == 0 ? (fp.x == 0 ? crn.x : fp.x == 1 ? mid.x : crn.y) : (fp.y == 1 ? (fp.x == 0 ? mid.w : fp.x == 1 ? 0. : mid.y) : (fp.x == 0 ? crn.w : fp.x == 1 ? mid.z : crn.z));
    ivec2 res = sp == 0. ? ivec2(0, 0) : sp == 1. ? ivec2(-1, 0) : sp == 2. ? ivec2(-2, 0) : sp == 3. ? ivec2(1, 0) : sp == 4. ? ivec2(2, 0) : sp == 5. ? ivec2(0, -1) : sp == 6. ? ivec2(0, -2) : sp == 7. ? ivec2(0, 1) : ivec2(0, 2);
    o_color = vec4(texelFetch(u_orig, clamp(s + res, ivec2(0), ivec2(u_size) - 1), 0).rgb, 1.0);
}
"#
    )
}

/// rAA-post3x: waagerecht (u_dir = (1,0)) oder senkrecht (u_dir = (0,1)) auf dem 3×-Bild
pub fn raa() -> String {
    format!(
        "{HEAD}{}",
        r#"
uniform ivec2 u_dir;
#define RAA_SHR 2.0
#define RAA_SMT 0.5
#define RAA_DVT 1.0
const int scl = 3;
const int rad = 7;
vec3 res2x(vec3 pre2, vec3 pre1, vec3 px, vec3 pos1, vec3 pos2) {
    float d1, d2, w;
    vec3 a, m, t, t1, t2;
    mat4x3 pre = mat4x3(pre2, pre1, px, pos1);
    mat4x3 pos = mat4x3(pre1, px, pos1, pos2);
    mat4x3 df = pos - pre;
    m.x = (px.x < 0.5) ? px.x : (1.0 - px.x);
    m.y = (px.y < 0.5) ? px.y : (1.0 - px.y);
    m.z = (px.z < 0.5) ? px.z : (1.0 - px.z);
    m = RAA_SHR * min(m, min(abs(df[1]), abs(df[2])));
    t = (7. * (df[1] + df[2]) - 3. * (df[0] + df[3])) / 16.;
    a.x = t.x == 0.0 ? 1.0 : m.x / abs(t.x);
    a.y = t.y == 0.0 ? 1.0 : m.y / abs(t.y);
    a.z = t.z == 0.0 ? 1.0 : m.z / abs(t.z);
    t1 = clamp(t, -m, m);
    t2 = min(1.0, min(min(a.x, a.y), a.z)) * t;
    d1 = length(df[1]); d2 = length(df[2]);
    d1 = d1 == 0.0 ? 0.0 : length(cross(df[1], t1)) / d1;
    d2 = d2 == 0.0 ? 0.0 : length(cross(df[2], t1)) / d2;
    w = min(1.0, max(d1, d2) / 0.8125);
    return mix(t1, t2, pow(w, RAA_DVT));
}
void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec3 tx[2 * rad + 1];
    #define TX(n) tx[(n) + rad]
    TX(0) = T(p).rgb;
    for (int i = 1; i <= rad; i++) {
        TX(-i) = T(p - u_dir * i).rgb;
        TX(i) = T(p + u_dir * i).rgb;
    }
    ivec2 i1 = ivec2(0), i2 = ivec2(0);
    vec3 df1, df2;
    vec2 d1, d2, d3;
    bvec2 cn;
    df1 = TX(1) - TX(0); df2 = TX(0) - TX(-1);
    d2 = vec2(length(df1), length(df2));
    d3 = d2.yx;
    float sw = d2.x + d2.y;
    sw = sw == 0.0 ? 1.0 : pow(length(df1 - df2) / sw, RAA_SMT);
    for (int i = 1; i < rad; i++) {
        d1 = d2;
        d2 = d3;
        d3 = vec2(distance(TX(-i - 1), TX(-i)), distance(TX(i), TX(i + 1)));
        cn.x = max(d1.x, d3.x) < d2.x;
        cn.y = max(d1.y, d3.y) < d2.y;
        i2.x = cn.x && i2.x == 0 && i1.x != 0 ? i : i2.x;
        i2.y = cn.y && i2.y == 0 && i1.y != 0 ? i : i2.y;
        i1.x = cn.x && i1.x == 0 ? i : i1.x;
        i1.y = cn.y && i1.y == 0 ? i : i1.y;
    }
    i2.x = i2.x == 0 ? i1.x + 1 : i2.x;
    i2.y = i2.y == 0 ? i1.y + 1 : i2.y;
    vec3 t = res2x(TX(-i2.x), TX(-i1.x), TX(0), TX(i1.y), TX(i2.y));
    // (bei i1 = (1, 1) wäre es 0/0 -> dann keine Verschiebung)
    float den = float(i1.x + i1.y) - 2.0;
    float dw = (i1.x == 0 || i1.y == 0 || den <= 0.0) ? 0.0 : 2.0 * ((float(i1.x) - 1.0) / den) - 1.0;
    vec3 res = TX(0) + (float(scl) - 1.0) / float(scl) * sw * dw * t;
    vec3 lo = min(min(TX(-1), TX(0)), TX(1));
    vec3 hi = max(max(TX(-1), TX(0)), TX(1));
    o_color = vec4(clamp(res, lo, hi), 1.0);
}
"#
    )
}
