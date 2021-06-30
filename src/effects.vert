// vim: ft=c

#version 100
attribute vec3 position;
attribute vec2 texcoord;
// attribute vec4 color0;

varying lowp vec2 uv;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    uv = texcoord;
}