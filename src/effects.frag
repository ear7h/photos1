// vim: ft=c

#version 100
precision highp float;

varying lowp vec2 uv;

uniform sampler2D Texture;

uniform lowp float brightness;
uniform lowp float contrast;
uniform int invert;

uniform float highlight;
uniform float shadow;
uniform float black_pt;
uniform float white_pt;

uniform float temperature;

#define PI 3.1415926535897932384626433832795

// f : [0, 1] -> [0, inf)
float unit2nnreal(float x) {
    return tan(PI * x / 2.);
    // return x < .5 ?  2. * x : 1. / (2.0001 - 2. * x);
}

vec3 linear2srgb(vec3 lin) {
    vec3 a = 12.92 * lin;
    vec3 b = 1.055 * pow(lin, vec3(1. / 2.4)) - 0.055;
    vec3 c = step(vec3(0.0031308), lin);
    return mix(a, b, c);
}

vec3 srgb2linear(vec3 srgb) {
    vec3 a = srgb / 12.92;
    vec3 b = pow((srgb + 0.055) / 1.055, vec3(2.4));
    vec3 c  = step(vec3(0.04045), srgb);
    return mix(a, b, c);
}

float luminance(vec3 color) {
    // return (color.r + color.b + color.g) / 3.;
    return 0.2126 * color.r + 0.7162 * color.g + 0.0722 * color.b;
}

// https://www.desmos.com/calculator/ghfukhxtng
float levels(float M, float mr, float th, float tl, float ah, float al, float lum) {

    float m = mix(tl, th, mr);
    float o = (lum - tl) / (th - tl) + M - .5;

    if (lum > .5) {
        float ph = pow((lum - m) / (th - m), 1. / 10.);
        float rah = tan(PI * (1. - ah) / 2.);
        float fh = (1. - M) / pow(th - m, rah) * pow(lum - m, rah) + M;
        return fh * ph + o * (1. - ph);
    } else {
        float pl = pow((tl - lum) / (m - tl) + 1., 1. / 10.);
        float ral = tan(PI * al / 2.);
        float fl = (-M) / pow(m - tl, ral) * pow(m - lum, ral) + M;
        return fl * pl + o * (1. - pl);
    }
}

// Valid from 1000 to 40000 K (and additionally 0 for pure full white)
// taken from: https://www.shadertoy.com/view/4sc3D7
// TODO: understand https://www.shadertoy.com/view/MtcfDr
vec3 kelvin2linear(float temperature){
    // Values from: https://blenderartists.org/t/osl-goodness/555304/322
    mat3 m = (temperature <= 6500.0) ?
        mat3(
            vec3(0.0, -2902.1955373783176   , -8257.7997278925690),
            vec3(0.0,  1669.5803561666639   ,  2575.2827530017594),
            vec3(1.0,     1.3302673723350029,     1.8993753891711275)
        ) : mat3(
            vec3( 1745.0425298314172    ,  1216.6168361476490    , -8257.7997278925690),
            vec3(-2666.3474220535695    , -2173.1012343082230    ,  2575.2827530017594),
            vec3(    0.55995389139931482,     0.70381203140554553,     1.8993753891711275)
        );

    return mix(
        clamp(vec3(m[0] / (vec3(clamp(temperature, 1000.0, 40000.0)) + m[1]) + m[2]), vec3(0.0), vec3(1.0)),
        vec3(1.0),
        smoothstep(1000.0, 0.0, temperature)
    );
}


void main() {
    lowp vec4 color = texture2D(Texture, uv);

    color.rgb = srgb2linear(color.rgb);

    // color correction described: https://en.wikipedia.org/wiki/Color_balance
    vec3 temp = kelvin2linear(temperature);
    mat3 monitorScale = mat3(
        vec3(1. / temp.r, 0., 0.),
        vec3(0., 1. / temp.g, 0.),
        vec3(0., 0., 1. / temp.b)
    );

    color.rgb = monitorScale * color.rgb;

    // contrast and brightness
    color = clamp(unit2nnreal(contrast) * (color - .5) + .5 + brightness, 0., 1.);


    float lum0 = luminance(color.rgb); // current luminance
    float lum = levels(.5, .5, white_pt, black_pt, highlight, shadow, lum0);

    color *= clamp(lum, 0., 1.)/lum0;

    if (invert > 0) {
        color = 1. - color;
    }

    gl_FragColor = vec4(linear2srgb(color.rgb), color.a);
}
