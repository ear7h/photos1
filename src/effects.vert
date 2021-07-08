// vim: ft=c

#version 100
precision lowp float;

attribute vec2 position;
attribute vec2 texcoord;

varying vec2 uv;

uniform mat4 matrix;

void main() {
    gl_Position = matrix * vec4(position, 0., 1.);
    uv = texcoord;
}
