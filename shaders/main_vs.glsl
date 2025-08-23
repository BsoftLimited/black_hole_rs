#version 330 core
    layout (location = 0) in vec2 aPos;  // Changed to vec2
    layout (location = 1) in vec2 aTexCoord;
    out vec2 TexCoord;
    void main() {
        gl_Position = vec4(aPos, 0.0, 1.0);  // Explicit z=0
        TexCoord = aTexCoord;
}