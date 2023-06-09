#version 460

#include "definitions.glsl"
#include "light_definitions.glsl"

layout(location=0) in vec2 uv;
layout(location=0) out vec4 color;

layout(set = 0, binding = 0) uniform sampler2D posSampler;
layout(set = 0, binding = 1) uniform sampler2D normSampler;
layout(set = 0, binding = 2) uniform sampler2D difSampler;
layout(set = 0, binding = 3) uniform sampler2D emissSampler;
layout(set = 0, binding = 4) uniform sampler2D pbrSampler;

layout(set = 0, binding = 5) uniform  PerFrameDataBlock {
    PerFrameData pfd;
} per_frame_data;

layout(set = 0, binding = 6, std140) readonly buffer LightData {
    uint light_count;
    LightInfo lights[];
} light_data;

struct FragmentInfo {
    vec3 diffuse;
    vec4 emissive;
    vec3 position;
    vec3 normal;
    float roughness;
    float metalness;
};

vec3 get_unnormalized_light_direction(LightInfo info, FragmentInfo frag_info) {
    if (info.type == DIRECTIONAL_LIGHT) {
        return info.direction.xyz;
    } else {
        return frag_info.position - info.position_radius.xyz ;
    }
}


FragmentInfo get_fragment_info(vec2 in_uv) {
    FragmentInfo info;
    info.diffuse = texture(difSampler, in_uv).rgb;
    info.emissive = texture(emissSampler, in_uv);
    info.position = texture(posSampler, in_uv).xyz;
    info.normal = texture(normSampler, in_uv).xyz;
    info.normal = info.normal * 2.0 - 1.0;
    
    vec4 pbr_sample = texture(pbrSampler, in_uv);
    info.metalness = pbr_sample.x;
    info.roughness = pbr_sample.y;

    return info;
}

float ggx_smith(float v_dot_n, float v_dot_l, float a)
{
    float r = a + 1.0;
    float k = (r * r) / 8.0;

    float gv = v_dot_n / (v_dot_n * (1.0 - k) + k);
    float gl = v_dot_l / (v_dot_l * (1.0 - k) + k);
    return gv * gl;
}

vec3 fresnel_schlick(float cos_theta, vec3 F0, vec3 F90)
{
    return F0 + (F90 - F0) * pow(1.0 - cos_theta, 5.0);
}

float d_trowbridge_reitz_ggx(float n_dot_h, float rough)
{
    float a = rough * rough;
    float n_dot_h_2 = n_dot_h * n_dot_h;
    float a_2 = a * a;
    float a_2_sub = a - 1.0;
    
    float d = n_dot_h_2 * a_2_sub + 1.0;
    return a_2 / (PI * d * d);
}

vec3 cook_torrance(vec3 view_direction, FragmentInfo frag_info, LightInfo light_info) {

    vec3 light_dir = get_unnormalized_light_direction(light_info, frag_info);
    float l_dot_n = max(dot(light_dir, frag_info.normal), 0.0);
    float light_dist = length(light_dir);
    light_dir /= light_dist;
    vec3 light_radiance = get_light_intensity(l_dot_n, light_dist, light_info);
    
    vec3 h = normalize(view_direction + light_dir);
    
    float v_dot_n = max(dot(view_direction, frag_info.normal), 0.0);
    float n_dot_h = max(dot(frag_info.normal, h), 0.0);
    float h_dot_v = max(dot(h, view_direction), 0.0);

    vec3 F0 = vec3(0.04);
    F0 = mix(F0, frag_info.diffuse, frag_info.metalness);
    
    // Reflective component
    float d = d_trowbridge_reitz_ggx(n_dot_h, frag_info.roughness);
    float g = ggx_smith(v_dot_n, l_dot_n, frag_info.roughness);
    vec3  f = fresnel_schlick(h_dot_v, F0, vec3(1.0));
    vec3 dfg = d * g * f;
    
    const float eps = 0.0001;
    return dfg * light_radiance;
    
    float denom = max(4.0 * (l_dot_n * v_dot_n), eps);
    vec3 s_cook_torrance = dfg / denom;
    
    // Refracftion component
    vec3 lambert = frag_info.diffuse / PI;
    vec3 ks = f;
    vec3 kd = mix(vec3(1.0) - f, vec3(0.0), frag_info.metalness);
    vec3 o = (kd * lambert + s_cook_torrance) * light_radiance * l_dot_n;
    return vec3(o);
}


vec3 calculate_light_influence(FragmentInfo frag_info) {
    vec3 ck = vec3(0.0);
    vec3 view = normalize(per_frame_data.pfd.eye.xyz - frag_info.position);
    
    for (uint i = 0; i < light_data.light_count; i ++) {
        ck += cook_torrance(view, frag_info, light_data.lights[i]);
    }
    
    return ck + 0.5 * frag_info.diffuse;
}

vec3 rgb(int r, int g, int b) {
    return vec3(
        255.0 / float(r),
        255.0 / float(g),
        255.0 / float(b)
    );
}

void main() {
    FragmentInfo fragInfo = get_fragment_info(uv);
    vec3 light_a = calculate_light_influence(fragInfo);
    color = vec4(light_a, 1.0) + fragInfo.emissive;
}