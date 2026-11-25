#version 330

#include <minecraft:dynamictransforms.glsl>
#include <sunrise:pipeline_sprite.glsl>

in vec2 texCoord0;
in vec4 vertexColor;

out vec4 fragColor;

void main() {
    fragColor = sampleSprite(texCoord0) * vertexColor * ColorModulator;
}
