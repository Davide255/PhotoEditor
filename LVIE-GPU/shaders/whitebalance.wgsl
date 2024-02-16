// implementation of LVIElib::white_balance.rs
@group(0) @binding(0) var input_texture : texture_2d<f32>;
@group(0) @binding(1) var output_texture : texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var <storage, read> parameters : array<f32>; 

let WP_U: array<f32, 6> = (
    0.860117757,
    1.54118254 * 0.0001,
    1.28641212 * 0.0000001,
    1.0,
    8.42420235 * 0.0001,
    7.08145163 * 0.0000001,
);

let WP_V: array<f32, 6> = (
    0.317398726,
    4.22806245 * 0.00001,
    4.20481691 * 0.00000001,
    1.0,
    -2.89741816 * 0.00001,
    1.61456053 * 0.0000001,
);

struct LinSrgb {
    r: f32,
    g: f32,
    b: f32
}

struct Xyz {
    x: f32,
    y: f32,
    z:f32
}

struct Lms {
    l: f32,
    m: f32,
    s: f32
}

fn rgb_to_linsrgb(rgb: vec3<f32>) -> LinSrgb {
    return LinSrgb(
        pow(rgb.r, 2.2),
        pow(rgb.g, 2.2),
        pow(rgb.b, 2.2)
    );
}

fn linsrgb_to_rgb(srgb: LinSrgb) -> vec3<f32> {
    return vec3<f32>(
        clamp(pow(srgb.r, 1.0 / 2.2), 0.0, 1.0),
        clamp(pow(srgb.g, 1.0 / 2.2), 0.0, 1.0),
        clamp(pow(srgb.b, 1.0 / 2.2), 0.0, 1.0)
    );
}

fn linsrgb_to_xyz(rgb: LinSrgb) -> Xyz {
    return Xyz(
        rgb.r * 0.4124564 + rgb.g * 0.3575761 + rgb.b * 0.1804375, 
        rgb.r * 0.2126729 + rgb.g * 0.7151522 + rgb.b * 0.0721750, 
        rgb.r * 0.0193339 + rgb.g * 0.1191920 + rgb.b * 0.9503041,
    );
}

fn xyz_to_linsrgb(xyz: Xyz) -> LinSrgb {
    return LinSrgb(
        xyz.x * 3.2404542 + xyz.y * -1.5371385 + xyz.z * -0.4985314,
        xyz.x * -0.9692660 + xyz.y * 1.8760108 + xyz.z * 0.0415560,
        xyz.x * 0.0556434 + xyz.y * -0.2040259 + xyz.z * 1.0572252,
    );
}

fn xyz_to_lms(xyz: Xyz) -> Lms {
    return Lms(
        xyz.x * 0.8951 + xyz.y * 0.2664 + xyz.z * -0.1614,
        xyz.x * -0.7502 + xyz.y * 1.7135 + xyz.z * 0.0367,
        xyz.x * 0.0389 + xyz.y * -0.0685 + xyz.z* 1.0296,
    );
}

fn lms_to_xyz(lms: Lms) -> Xyz {
    return Xyz(
        lms.l * 0.986993 + lms.m * -0.147054 + lms.s * 0.159963,
        lms.l * 0.432305 + lms.m * 0.51836 + lms.s * 0.0492912,
        lms.l * -0.00852866 + lms.m * 0.0400428 + lms.s * 0.968487,
    );
}

fn normalize_2d(x: f32, y: f32) -> vec2<f32> {
    let norm = sqrt(x * x + y * y);

    return vec2<f32>(x / norm, y / norm);
}

fn uv_white_point(temp: f32, tint: f32) -> vec2<f32> {
    // Planck's locus in uv chromacity coordinates
    let u = (WP_U[0] + WP_U[1] * temp + WP_U[2] * temp * temp)
        / (WP_U[3] + WP_U[4] * temp + WP_U[5] * temp * temp);
    let v = (WP_V[0] + WP_V[1] * temp + WP_V[2] * temp * temp)
        / (WP_V[3] + WP_V[4] * temp + WP_V[5] * temp * temp);

    // derivatives of the parametric equations, for calculating the normal vector and moving on the isothermal line
    let (a, b, c, d, f, g, t) = (WP_U[0], WP_U[1], WP_U[2], WP_U[3], WP_U[4], WP_U[5], temp);
    let du = pow((-a * (f + 2.0 * g * t) + b * (d - g * t * t) + c * t * (2.0 * d + f * t))
        / (d + t * (f + g * t)),2);

    let (a, b, c, d, f, g, t) = (WP_V[0], WP_V[1], WP_V[2], WP_V[3], WP_V[4], WP_V[5], temp);
    let dv = pow((-a * (f + 2.0 * g * t) + b * (d - g * t * t) + c * t * (2.0 * d + f * t))
        / (d + t * (f + g * t)), 2);

    let normalized = normalize_2d(du, dv);
    du = normalized[0];
    dv = normalized[1];

    return vec2<f32>(u + tint * dv / 1000.0, v - tint * du / 1000.0);
}

fn uv_to_xy(u: f32, v: f32) -> vec2<f32> {
    return vec2<f32>
        (
            3.0 * u / (2.0 * u - 8.0 * v + 4.0),
            2.0 * v / (2.0 * u - 8.0 * v + 4.0),
        );
}

fn xy_white_point(temp: f32) -> vec2<f32> {
    if (temp < 0.0) {
        let x = (-4607000000.0 / (temp * temp * temp))
            + (2967800.0 / (temp * temp))
            + 99.11 / temp
            + 0.244063;
    } else {
        let x = (-2006400000.0 / (temp * temp * temp))
            + (1901800.0 / (temp * temp))
            + 247.48 / temp
            + 0.237040;
    };

    let y = -3.0 * x * x + 2.87 * x - 0.275;

    return vec2<f32>(x, y);
}

fn xyz_wb_matrix(fromtemp: f32, fromtint: f32, totemp: f32, totint: f32) -> matrix3x3<f32> {

    let uv = uv_white_point(fromtemp, fromtint);
    let xy = uv_to_xy(uv[0], uv[1]);
    let x = xy[0];
    let y = xy[1];
    //let (x, y) = (0.31271, 0.32902);
    let fromwp_xyz = Xyz(x / y, 1.0, (1.0 - x - y) / y);
    let fromwp = xyz_to_lms(fromwp_xyz);

    let uv = uv_white_point(totemp, totint);
    let xy = uv_to_xy(uv[0], uv[1]);
    let x = xy[0];
    let y = xy[1];
    //let (x, y) = (0.28315, 0.29711);
    let towp_xyz = Xyz(x / y, 1.0, (1.0 - x - y) / y);
    let towp = xyz_to_lms(towp_xyz);

    // inverted column-major from chat GPT
    let xyz_to_lms_transposed = matrix3x3<f32>(
        0.8951, -0.7502, 0.0389,
        0.2664, 1.7135, -0.0685,
        -0.1614, 0.0367, 1.0296,
    );

    let lms_to_xyz_transposed = matrix3x3<f32>(
        0.986993, 0.432305, -0.00852866,
        -0.147054, 0.51836, 0.0400428,
        0.159963, 0.0492912, 0.968487,
    );

    let diag = matrix3x3<f32>(
        fromwp[0] / towp[0], 0.0, 0.0,
        0.0, fromwp[1] / towp[1], 0.0,
        0.0, 0.0, fromwp[2] / towp[2],
    );

    return (lms_to_xyz * diag) * xyz_to_lms;
}

@compute @workgroup_size(16, 16)
fn shader_main(
  @builtin(global_invocation_id) global_id : vec3<u32>,
) {
    let dimensions = textureDimensions(input_texture);
    let coords = vec2<i32>(global_id.xy);

    if(coords.x >= dimensions.x || coords.y >= dimensions.y) {
        return;
    }

    let xyz_color = linsrgb_to_xyz(rgb_to_linsrgb(textureLoad(input_texture, coords.xy, 0)));

    var downscaled = Xyz(xyz.x / xyz.y, 1, xyz.z / xyz.y);

    downscaled = downscaled * xyz_wb_matrix(parameters[0], parameters[1], parameters[2], parameters[3]);

    let downscaled_lms = xyz_to_lms(downscaled);
}
