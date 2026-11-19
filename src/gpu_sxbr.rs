//! „HD Super“: Super-xBR (2× + 2× = 4×), portiert aus libretro/glsl-shaders (super-xbr-6p).
//!
//! Super-xBR: Copyright (c) 2015 Hyllian (sergiogdb@gmail.com), MIT-Lizenz:
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
//! associated documentation files (the "Software"), to deal in the Software without restriction,
//! including without limitation the rights to use, copy, modify, merge, publish, distribute,
//! sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions: The above copyright notice and this
//! permission notice shall be included in all copies or substantial portions of the Software.
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
//!
//! Ablauf je 2×: Pass 0 berechnet die Diagonal-Mitten, Pass 1 setzt das 2×-Gitter zusammen und füllt die
//! übrigen Punkte entlang der stärksten Kante, Pass 2 glättet nach. Alle Durchgänge arbeiten mit
//! gl_FragCoord/texelFetch – Zeile 0 ist überall die oberste Bildzeile.

/// gemeinsamer Teil: Quelle lesen und der eigentliche Super-xBR-Kern (Modus „Normal“)
const CORE: &str = r#"
out vec4 o_color;
uniform sampler2D u_tex;
uniform vec2 u_size;
vec3 T(ivec2 q) { return texelFetch(u_tex, clamp(q, ivec2(0), ivec2(u_size) - 1), 0).rgb; }
const vec3 Yc = vec3(.2126, .7152, .0722);
float lum(vec3 c) { return dot(c, Yc); }
float df(float A, float B) { return abs(A - B); }
float d_wd(float wp1, float wp2, float wp3, float wp4, float wp5, float wp6, float b0, float b1, float c0, float c1, float c2, float d0, float d1, float d2, float d3, float e1, float e2, float e3, float f2, float f3) {
    return (wp1 * (df(c1, c2) + df(c1, c0) + df(e2, e1) + df(e2, e3)) + wp2 * (df(d2, d3) + df(d0, d1)) + wp3 * (df(d1, d3) + df(d0, d2)) + wp4 * df(d1, d2) + wp5 * (df(c0, c2) + df(e1, e3)) + wp6 * (df(b0, b1) + df(f2, f3)));
}
float hv_wd(float wp1, float wp2, float wp3, float wp4, float wp5, float wp6, float i1, float i2, float i3, float i4, float e1, float e2, float e3, float e4) {
    return (wp4 * (df(i1, i2) + df(i3, i4)) + wp1 * (df(i1, e1) + df(i2, e2) + df(i3, e3) + df(i4, e4)) + wp3 * (df(i1, e2) + df(i3, e4) + df(e1, i2) + df(e3, i4)));
}
vec3 min4(vec3 a, vec3 b, vec3 c, vec3 d) { return min(a, min(b, min(c, d))); }
vec3 max4(vec3 a, vec3 b, vec3 c, vec3 d) { return max(a, max(b, max(c, d))); }
vec3 xbr(float wp1, float wp2, float wp3, float wp4, float wp5, float wp6, float weight1, float weight2,
         vec3 P0, vec3 P1, vec3 P2, vec3 P3, vec3 B, vec3 C, vec3 D, vec3 E, vec3 F, vec3 G, vec3 H, vec3 I,
         vec3 F4, vec3 I4, vec3 H5, vec3 I5) {
    float b = lum(B), c = lum(C), d = lum(D), e = lum(E), f = lum(F), g = lum(G), h = lum(H), i = lum(I);
    float i4 = lum(I4), p0 = lum(P0), i5 = lum(I5), p1 = lum(P1), h5 = lum(H5), p2 = lum(P2), f4 = lum(F4), p3 = lum(P3);
    float d_edge = (d_wd(wp1, wp2, wp3, wp4, wp5, wp6, d, b, g, e, c, p2, h, f, p1, h5, i, f4, i5, i4) - d_wd(wp1, wp2, wp3, wp4, wp5, wp6, c, f4, b, f, i4, p0, e, i, p3, d, h, i5, g, h5));
    float hv_edge = (hv_wd(wp1, wp2, wp3, wp4, wp5, wp6, f, i, e, h, c, i5, b, h5) - hv_wd(wp1, wp2, wp3, wp4, wp5, wp6, e, f, h, i, d, f4, g, i4));
    float edge_strength = smoothstep(0.0, 0.6 + 0.000001, abs(d_edge));
    vec4 w1 = vec4(-weight1, weight1 + 0.5, weight1 + 0.5, -weight1);
    vec4 w2 = vec4(-weight2, weight2 + 0.25, weight2 + 0.25, -weight2);
    vec3 c3 = mat4x3(D + G, E + H, F + I, F4 + I4) * w2;
    vec3 c4 = mat4x3(C + B, F + E, I + H, I5 + H5) * w2;
    vec3 c1 = mat4x3(P2, H, F, P1) * w1;
    vec3 c2 = mat4x3(P0, E, I, P3) * w1;
    vec3 color = mix(mix(c1, c2, step(0.0, d_edge)), mix(c3, c4, step(0.0, hv_edge)), 1. - edge_strength);
    // gegen Überschwinger: auf die vier nächsten Farben begrenzen
    return clamp(color, min4(E, F, H, I), max4(E, F, H, I));
}
"#;

/// Pass 0: Diagonal-Mitte zwischen E, F, H, I (gleiche Größe wie die Quelle)
pub fn pass0() -> String {
    format!(
        "{CORE}{}",
        r#"
void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec3 P0 = T(p + ivec2(-1, -1)), P1 = T(p + ivec2(2, -1)), P2 = T(p + ivec2(-1, 2)), P3 = T(p + ivec2(2, 2));
    vec3 B = T(p + ivec2(0, -1)), C = T(p + ivec2(1, -1)), H5 = T(p + ivec2(0, 2)), I5 = T(p + ivec2(1, 2));
    vec3 D = T(p + ivec2(-1, 0)), F4 = T(p + ivec2(2, 0)), G = T(p + ivec2(-1, 1)), I4 = T(p + ivec2(2, 1));
    vec3 E = T(p), F = T(p + ivec2(1, 0)), H = T(p + ivec2(0, 1)), I = T(p + ivec2(1, 1));
    o_color = vec4(xbr(2., 1., -1., 4., -1., 1., 1.29633 / 10., 1.75068 / 10. / 2., P0, P1, P2, P3, B, C, D, E, F, G, H, I, F4, I4, H5, I5), 1.0);
}
"#
    )
}

/// Pass 1: 2×-Gitter. Gerade/gerade = Original, ungerade/ungerade = Diagonal-Mitte (u_tex),
/// gemischt = entlang der Kante berechnet. u_orig = Original, u_size = Größe der Quelle (1×).
pub fn pass1() -> String {
    format!(
        "{CORE}{}",
        r#"
uniform sampler2D u_orig;
vec3 O(ivec2 L) { return texelFetch(u_orig, clamp(L >> 1, ivec2(0), ivec2(u_size) - 1), 0).rgb; }
vec3 S(ivec2 L) { return texelFetch(u_tex, clamp(L >> 1, ivec2(0), ivec2(u_size) - 1), 0).rgb; }
void main() {
    ivec2 X = ivec2(gl_FragCoord.xy);
    bool ox = (X.x & 1) == 1, oy = (X.y & 1) == 1;
    if (ox == oy) {
        o_color = vec4(ox ? S(X) : O(X), 1.0);
        return;
    }
    ivec2 g1 = ox ? ivec2(1, 0) : ivec2(0, 1);
    ivec2 g2 = ox ? ivec2(0, 1) : ivec2(1, 0);
    vec3 P0 = O(X - 3 * g1), P1 = S(X - 3 * g2), P2 = S(X + 3 * g2), P3 = O(X + 3 * g1);
    vec3 B = S(X - 2 * g1 - g2), C = O(X - g1 - 2 * g2), D = S(X - 2 * g1 + g2), E = O(X - g1);
    vec3 F = S(X - g2), G = O(X - g1 + 2 * g2), H = S(X + g2), I = O(X + g1);
    vec3 F4 = O(X + g1 - 2 * g2), I4 = S(X + 2 * g1 - g2), H5 = O(X + g1 + 2 * g2), I5 = S(X + 2 * g1 + g2);
    o_color = vec4(xbr(8., 0., 0., 0., 0., 0., 1.75068 / 10., 1.29633 / 10. / 2., P0, P1, P2, P3, B, C, D, E, F, G, H, I, F4, I4, H5, I5), 1.0);
}
"#
    )
}

/// Pass 2: Nachglätten auf dem 2×-Bild (gleiche Größe)
pub fn pass2() -> String {
    format!(
        "{CORE}{}",
        r#"
void main() {
    ivec2 p = ivec2(gl_FragCoord.xy);
    vec3 P0 = T(p + ivec2(-2, -2)), P1 = T(p + ivec2(1, -2)), P2 = T(p + ivec2(-2, 1)), P3 = T(p + ivec2(1, 1));
    vec3 B = T(p + ivec2(-1, -2)), C = T(p + ivec2(0, -2)), H5 = T(p + ivec2(-1, 1)), I5 = T(p + ivec2(0, 1));
    vec3 D = T(p + ivec2(-2, -1)), F4 = T(p + ivec2(1, -1)), G = T(p + ivec2(-2, 0)), I4 = T(p + ivec2(1, 0));
    vec3 E = T(p + ivec2(-1, -1)), F = T(p + ivec2(0, -1)), H = T(p + ivec2(-1, 0)), I = T(p);
    o_color = vec4(xbr(1., 0., 2., 3., -2., 1., 1.29633 / 10., 1.75068 / 10. / 2., P0, P1, P2, P3, B, C, D, E, F, G, H, I, F4, I4, H5, I5), 1.0);
}
"#
    )
}
