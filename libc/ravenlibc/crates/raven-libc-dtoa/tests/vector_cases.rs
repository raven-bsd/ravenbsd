// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One test per selected golden vector (the spread and the hard tail of
//! `tests/vectors/golden.txt`), so a failure names the exact case.

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, RangeFlags, Rounding, strtod};

fn check(hex_input: &str, bits: u64, consumed: usize, erange: bool) {
    let input: Vec<u8> = (0..hex_input.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_input[i..i + 2], 16).expect("hex"))
        .collect();
    let mut ctx = DtoaCtx::new();
    let parsed = strtod(&mut ctx, &input, &C_NUMERIC, Rounding::Nearest).expect("strtod");
    assert_eq!(
        parsed.value.to_bits(),
        bits,
        "{:?}",
        String::from_utf8_lossy(&input)
    );
    assert_eq!(
        parsed.consumed,
        consumed,
        "{:?}",
        String::from_utf8_lossy(&input)
    );
    let got =
        parsed.range.contains(RangeFlags::OVERFLOW) || parsed.range.contains(RangeFlags::UNDERFLOW);
    assert_eq!(got, erange, "{:?}", String::from_utf8_lossy(&input));
}

#[test]
fn golden_vector_000() {
    check(
        "2d312e39323530363930393436333533343535652b313131",
        0xd7099d7e5705c4ec,
        24,
        false,
    );
}

#[test]
fn golden_vector_001() {
    check(
        "2d382e32323134353037383935343831343938652d323839",
        0x8420062cfc209e5a,
        24,
        false,
    );
}

#[test]
fn golden_vector_002() {
    check(
        "353134383236362e363932363537313836",
        0x4153a39aac547ece,
        17,
        false,
    );
}

#[test]
fn golden_vector_003() {
    check(
        "362e31343732333136393530323233393235652d313335",
        0x2411df4776419219,
        23,
        false,
    );
}

#[test]
fn golden_vector_004() {
    check(
        "2d352e39313339333539383231323339353736652d313234",
        0xa659053b1b685255,
        24,
        false,
    );
}

#[test]
fn golden_vector_005() {
    check(
        "352e32383930373335393132323832393431652b3930",
        0x52c4c58a8b98111f,
        22,
        false,
    );
}

#[test]
fn golden_vector_006() {
    check(
        "2d352e38343031303735303431333235323835652b313431",
        0xdd5ea69fad001c09,
        24,
        false,
    );
}

#[test]
fn golden_vector_007() {
    check(
        "2d312e36343939373234373935353039383338652b3435",
        0xc9527f3054988d6a,
        23,
        false,
    );
}

#[test]
fn golden_vector_008() {
    check(
        "382e36323738373031383134363739393538652b323635",
        0x772567f5ba6736a6,
        23,
        false,
    );
}

#[test]
fn golden_vector_009() {
    check(
        "2d322e33353133393934363139383335343933652b3934",
        0xd3868b9eaf933c55,
        23,
        false,
    );
}

#[test]
fn golden_vector_010() {
    check(
        "2d382e383034383733373536383430313131652b3338",
        0xc804b33d9a1bdfa1,
        22,
        false,
    );
}

#[test]
fn golden_vector_011() {
    check(
        "2d342e313831323730343233343532363332652d3039",
        0xbe31f55afebc66e1,
        22,
        false,
    );
}

#[test]
fn golden_vector_012() {
    check(
        "2d392e3635383638313036343638353936652d313633",
        0x9e4bcf6ef87778fa,
        22,
        false,
    );
}

#[test]
fn golden_vector_013() {
    check(
        "2d372e37313336313730383433393632333533652b3536",
        0xcbbf75645a606d2d,
        23,
        false,
    );
}

#[test]
fn golden_vector_014() {
    check(
        "332e36383538303134393737363432353636652b323634",
        0x76dd434635f14b36,
        23,
        false,
    );
}

#[test]
fn golden_vector_015() {
    check(
        "332e36383337323634353430353837303332652b323131",
        0x6bdc032cfc57392d,
        23,
        false,
    );
}

#[test]
fn golden_vector_016() {
    check(
        "342e37373538393637313332383537373633652d313736",
        0x1b8831049f123817,
        23,
        false,
    );
}

#[test]
fn golden_vector_017() {
    check(
        "2d322e33313834343131353133393135313736652d313133",
        0xa88c8c193e714dce,
        24,
        false,
    );
}

#[test]
fn golden_vector_018() {
    check(
        "382e363532333437333837313839363538652d323039",
        0x14bc7200f27914b2,
        22,
        false,
    );
}

#[test]
fn golden_vector_019() {
    check(
        "2d342e31333136313833303532353236383735652d323238",
        0x90b90e6983eecf86,
        24,
        false,
    );
}

#[test]
fn golden_vector_020() {
    check(
        "332e393330393735393134333338373936652d323337",
        0x0ed998f376e68b02,
        22,
        false,
    );
}

#[test]
fn golden_vector_021() {
    check(
        "2d342e3533353136373537353537343031652d313636",
        0x9d9abe3886e2c479,
        22,
        false,
    );
}

#[test]
fn golden_vector_022() {
    check(
        "2d372e39323836333436383930363732373431652b323633",
        0xf6b92de10bbca6be,
        24,
        false,
    );
}

#[test]
fn golden_vector_023() {
    check(
        "322e30313138353336373436393731313633652d313733",
        0x1c13e75915d192b5,
        23,
        false,
    );
}

#[test]
fn golden_vector_024() {
    check(
        "2d322e35313733343835353838373538383532652b313334",
        0xdbd62a7816ae8cd7,
        24,
        false,
    );
}

#[test]
fn golden_vector_025() {
    check(
        "322e31313833383635333831393634323533652d323737",
        0x067e0a9de0288ad2,
        23,
        false,
    );
}

#[test]
fn golden_vector_026() {
    check(
        "312e36373139313032373239303438313139652b313839",
        0x6738040671ef2d91,
        23,
        false,
    );
}

#[test]
fn golden_vector_027() {
    check(
        "2d32363838303031352e3236393234393337",
        0xc179a280f44ed86d,
        18,
        false,
    );
}

#[test]
fn golden_vector_028() {
    check(
        "2d392e353836373936383339373230373736652d313930",
        0x98b115eb4c113444,
        23,
        false,
    );
}

#[test]
fn golden_vector_029() {
    check(
        "352e343532323831323932363633323934652d313434",
        0x2231054966000c70,
        22,
        false,
    );
}

#[test]
fn golden_vector_030() {
    check(
        "352e383331343038363439353137383239652d323231",
        0x1235143a3d4fc0b3,
        22,
        false,
    );
}

#[test]
fn golden_vector_031() {
    check(
        "312e32323538323232393235313939333133652d313438",
        0x21391426fc45766c,
        23,
        false,
    );
}

#[test]
fn golden_vector_032() {
    check(
        "312e31343532373035393336333530353932652b3138",
        0x43afc9a312f2a30c,
        22,
        false,
    );
}

#[test]
fn golden_vector_033() {
    check(
        "2d332e313838343433303334313732333035652b3738",
        0xd03b8932ce0eaf0c,
        22,
        false,
    );
}

#[test]
fn golden_vector_034() {
    check(
        "322e38373335333939393337323435303935652b3834",
        0x5177aaaf7bc50ee2,
        22,
        false,
    );
}

#[test]
fn golden_vector_035() {
    check(
        "382e34303831313936333633363839393535652d313034",
        0x2a881ae2291ad2fb,
        23,
        false,
    );
}

#[test]
fn golden_vector_036() {
    check(
        "2d332e33313231343839383836353531353535652d323630",
        0x8a104bd32e488590,
        24,
        false,
    );
}

#[test]
fn golden_vector_037() {
    check(
        "2d352e31363236393136363136323036343739652d313836",
        0x9976769aa221bd9c,
        24,
        false,
    );
}

#[test]
fn golden_vector_038() {
    check(
        "332e383133363134323032373134343038652b3136",
        0x4360ef93cfd43772,
        21,
        false,
    );
}

#[test]
fn golden_vector_039() {
    check(
        "322e353737343334343039313639393339652b323430",
        0x71d8bd05c1d563f8,
        22,
        false,
    );
}

#[test]
fn golden_vector_040() {
    check(
        "2d362e383931323430363336393333393737652b3737",
        0xd017ce39977187cb,
        22,
        false,
    );
}

#[test]
fn golden_vector_041() {
    check(
        "392e363239323836333535373430333632652d323533",
        0x0b9c3d33f3a5a9b9,
        22,
        false,
    );
}

#[test]
fn golden_vector_042() {
    check(
        "362e37313630303836313638353136373035652d313939",
        0x16c9b43041e4e7c4,
        23,
        false,
    );
}

#[test]
fn golden_vector_043() {
    check(
        "372e34383831343735313637353237343739652d323834",
        0x0526451efa7122bb,
        23,
        false,
    );
}

#[test]
fn golden_vector_044() {
    check(
        "2d322e373032343535383339363333313337652b323435",
        0xf2e3ca149ca9c176,
        23,
        false,
    );
}

#[test]
fn golden_vector_045() {
    check(
        "342e333731303438383837393833383932652b323435",
        0x72f00107b1cb72fa,
        22,
        false,
    );
}

#[test]
fn golden_vector_046() {
    check(
        "322e38363630363735343738343838353037652b333030",
        0x7e511e62feda342a,
        23,
        false,
    );
}

#[test]
fn golden_vector_047() {
    check(
        "2d342e39323132333936393231313939353237652d313435",
        0xa1f894a1fa8ea58c,
        24,
        false,
    );
}

#[test]
fn golden_vector_048() {
    check(
        "2d392e393338393738353537343634313934652d313039",
        0xa982ac770bc40040,
        23,
        false,
    );
}

#[test]
fn golden_vector_049() {
    check(
        "2d312e38353333383235363030323433393031652b3436",
        0xc989f8af11d8205f,
        23,
        false,
    );
}

#[test]
fn golden_vector_050() {
    check(
        "342e383237343035313136353335313831652d323339",
        0x0e741e45eb1780e0,
        22,
        false,
    );
}

#[test]
fn golden_vector_051() {
    check(
        "392e30363439323739333138303835313631652b3535",
        0x4b8d93618edc958a,
        22,
        false,
    );
}

#[test]
fn golden_vector_052() {
    check(
        "2d3334313331323131303537383939312e3338",
        0xc2f366beb1bcd2f6,
        19,
        false,
    );
}

#[test]
fn golden_vector_053() {
    check(
        "352e393534373932323532313936303733652d313432",
        0x229d0bbee1c1fe19,
        22,
        false,
    );
}

#[test]
fn golden_vector_054() {
    check(
        "312e38353932303537333133353337323031652d333037",
        0x0040b61ec3300312,
        23,
        false,
    );
}

#[test]
fn golden_vector_055() {
    check(
        "322e34353334353532393137373530373033652b313334",
        0x5bd59a71c3fa7b08,
        23,
        false,
    );
}

#[test]
fn golden_vector_056() {
    check(
        "2d312e363032363035393534393534303438652b323932",
        0xfc99b1f482c0093a,
        23,
        false,
    );
}

#[test]
fn golden_vector_057() {
    check(
        "372e37303039343431323530383836323034652b313931",
        0x67c59ae9417f7df9,
        23,
        false,
    );
}

#[test]
fn golden_vector_058() {
    check(
        "2d342e34313433303239353339333232363532652b3630",
        0xcc85f9e98a17683b,
        23,
        false,
    );
}

#[test]
fn golden_vector_059() {
    check(
        "37313339393635303738312e323130343634",
        0x42309fc035dd35e1,
        18,
        false,
    );
}

#[test]
fn golden_vector_060() {
    check(
        "362e363631313233313230323430343131652d3531",
        0x3583f0121bfb427f,
        21,
        false,
    );
}

#[test]
fn golden_vector_061() {
    check(
        "332e383338353430393931343237363437652b333034",
        0x7f2bfcbdc68c830e,
        22,
        false,
    );
}

#[test]
fn golden_vector_062() {
    check(
        "312e32363932373230333032343135303634652b3934",
        0x537856f99b85e814,
        22,
        false,
    );
}

#[test]
fn golden_vector_063() {
    check(
        "2d372e373739323834333530373038323532652d323937",
        0x827459bb38ea501e,
        23,
        false,
    );
}

#[test]
fn golden_vector_064() {
    check(
        "2d322e39323334323830333733323238343734652b3839",
        0xd2825ea9226c273c,
        23,
        false,
    );
}

#[test]
fn golden_vector_065() {
    check(
        "2d322e31303433333933303738363931303336652d313032",
        0xaad2da4c9e47d9b9,
        24,
        false,
    );
}

#[test]
fn golden_vector_066() {
    check(
        "342e39303335373933333339393036353435652d313530",
        0x20f00d23ff8185ff,
        23,
        false,
    );
}

#[test]
fn golden_vector_067() {
    check(
        "322e373339303130313033383035313437652d3139",
        0x3c1435d8038cfe97,
        21,
        false,
    );
}

#[test]
fn golden_vector_068() {
    check(
        "2d312e33333138363931373536303139313036652b3532",
        0xcac1cc8264df55ca,
        23,
        false,
    );
}

#[test]
fn golden_vector_069() {
    check(
        "392e303936313935313336383939303733652b323930",
        0x7c5755b4f2d98008,
        22,
        false,
    );
}

#[test]
fn golden_vector_070() {
    check(
        "2d382e39393731313131313133303330383736652b323535",
        0xf5132cb3db475038,
        24,
        false,
    );
}

#[test]
fn golden_vector_071() {
    check(
        "2d332e37343938323230353335313437303935652d323934",
        0x830328be9075ebf4,
        24,
        false,
    );
}

#[test]
fn golden_vector_072() {
    check(
        "2d332e31393233303239353633323536363833652b313930",
        0xe77ca8cd445e03a8,
        24,
        false,
    );
}

#[test]
fn golden_vector_073() {
    check(
        "312e33393939313330313138333130393034652b3430",
        0x484491e331cb7ff0,
        22,
        false,
    );
}

#[test]
fn golden_vector_074() {
    check(
        "322e33383932353535343434363833333432652d3038",
        0x3e59a7891f30bf51,
        22,
        false,
    );
}

#[test]
fn golden_vector_075() {
    check(
        "2d312e393030393438373734333738303036652b313433",
        0xddaf2d7622f16ec9,
        23,
        false,
    );
}

#[test]
fn golden_vector_076() {
    check(
        "2d332e33313831303630363539333539343336652b323933",
        0xfce0a007139ff748,
        24,
        false,
    );
}

#[test]
fn golden_vector_077() {
    check(
        "342e34343738323938353334363235343433652d323735",
        0x06f8a399a76218d6,
        23,
        false,
    );
}

#[test]
fn golden_vector_078() {
    check(
        "312e30323036393437303139313131323937652d3333",
        0x391532f1a2e33db8,
        22,
        false,
    );
}

#[test]
fn golden_vector_079() {
    check(
        "312e39323237353431373132393535313237652b3337",
        0x47acee2dfebf563a,
        22,
        false,
    );
}

#[test]
fn golden_vector_080() {
    check(
        "332e333736393838353730323937393136652b3633",
        0x4d206b02a293a9ef,
        21,
        false,
    );
}

#[test]
fn golden_vector_081() {
    check(
        "322e31353137373535313837373837313936652b3436",
        0x498e271db94ffa85,
        22,
        false,
    );
}

#[test]
fn golden_vector_082() {
    check(
        "2d362e303138393838393136363731303231652d3134",
        0xbd30f1237b3560fb,
        22,
        false,
    );
}

#[test]
fn golden_vector_083() {
    check(
        "312e32303636383534333039373838363031652d313238",
        0x2560ba8c1fc4eab2,
        23,
        false,
    );
}

#[test]
fn golden_vector_084() {
    check(
        "2d322e323132323232363931363038333838652b3434",
        0xc923d703a36c6e34,
        22,
        false,
    );
}

#[test]
fn golden_vector_085() {
    check(
        "372e35383331323532313632363935303239652b313836",
        0x66bbe29068ea4b5a,
        23,
        false,
    );
}

#[test]
fn golden_vector_086() {
    check(
        "342e373131343933353435313236383534652b323930",
        0x7c482c51cf1acda4,
        22,
        false,
    );
}

#[test]
fn golden_vector_087() {
    check(
        "2d312e31353531353030323131383835343939652d313738",
        0x9afdf54d00f2801f,
        24,
        false,
    );
}

#[test]
fn golden_vector_088() {
    check(
        "2d372e323535353533383532303737383232652d3232",
        0xbb8b69241fc75b27,
        22,
        false,
    );
}

#[test]
fn golden_vector_089() {
    check(
        "322e32393333373535323539323038363339652d323633",
        0x09671be82f6b4388,
        23,
        false,
    );
}

#[test]
fn golden_vector_090() {
    check(
        "2d322e393332373337353036343436353039652b3834",
        0xd178277ff6efb212,
        22,
        false,
    );
}

#[test]
fn golden_vector_091() {
    check(
        "2d312e303839363036313130383638313032652d313039",
        0xa95060a4ab15cc34,
        23,
        false,
    );
}

#[test]
fn golden_vector_092() {
    check(
        "2d332e36303836333431303633363235363333652d323637",
        0x889dc9afc87e6f59,
        24,
        false,
    );
}

#[test]
fn golden_vector_093() {
    check(
        "2d312e323036373734393636343531383738652d3230",
        0xbbcc7e81cfcd0cca,
        22,
        false,
    );
}

#[test]
fn golden_vector_094() {
    check(
        "332e33393839393332303632303336373239652d313036",
        0x2a08f224ec09df34,
        23,
        false,
    );
}

#[test]
fn golden_vector_095() {
    check(
        "342e333930383435333638313830323138652d313735",
        0x1bbbcd241f1ff36a,
        22,
        false,
    );
}

#[test]
fn golden_vector_096() {
    check(
        "352e33303430303731313632323739393832652d313939",
        0x16c44cbe0d282f29,
        23,
        false,
    );
}

#[test]
fn golden_vector_097() {
    check(
        "2d362e31313134373430303736393038303137652d323838",
        0x844dc77430231b3b,
        24,
        false,
    );
}

#[test]
fn golden_vector_098() {
    check(
        "2d352e37343736373236333133333232373637652d323634",
        0x89472a98024a406c,
        24,
        false,
    );
}

#[test]
fn golden_vector_099() {
    check(
        "2d352e39303134323930373537393330333138652b323633",
        0xf6b2bdc966c4095e,
        24,
        false,
    );
}

#[test]
fn golden_vector_100() {
    check(
        "2d342e38343232383435363832363833343338652b323333",
        0xf0737e614ab5b31b,
        24,
        false,
    );
}

#[test]
fn golden_vector_101() {
    check(
        "323537323232393034353430323938322e35",
        0x432246db7d107acd,
        18,
        false,
    );
}

#[test]
fn golden_vector_102() {
    check(
        "2d312e32323638303336363236363037333233652d323737",
        0x867165cd9e5c8728,
        24,
        false,
    );
}

#[test]
fn golden_vector_103() {
    check(
        "382e3831373437303133363933383731652d323234",
        0x11a051a3265180c5,
        21,
        false,
    );
}

#[test]
fn golden_vector_104() {
    check(
        "2d382e3138353632323338313031333235652d323239",
        0x9093db54ab333849,
        22,
        false,
    );
}

#[test]
fn golden_vector_105() {
    check(
        "372e343632323437363136333931363536652b323331",
        0x701339e6daefac3b,
        22,
        false,
    );
}

#[test]
fn golden_vector_106() {
    check(
        "392e35353532383830363130313135383832652b313733",
        0x640ee82e6e4c2618,
        23,
        false,
    );
}

#[test]
fn golden_vector_107() {
    check(
        "2d342e32363437373530313530303436303234652b3430",
        0xc85f55267a1404f7,
        23,
        false,
    );
}

#[test]
fn golden_vector_108() {
    check(
        "2d332e343238373339323332383831323939652d3535",
        0xb4a0d0824dadb95d,
        22,
        false,
    );
}

#[test]
fn golden_vector_109() {
    check(
        "332e37343339373230343033353836363836652d3935",
        0x2c53fe20e3b0ae6a,
        22,
        false,
    );
}

#[test]
fn golden_vector_110() {
    check(
        "322e38333633353137333937393739363531652b313132",
        0x5747968930bfb4f2,
        23,
        false,
    );
}

#[test]
fn golden_vector_111() {
    check(
        "312e3435313637363430323135303238652d313735",
        0x1ba2620ec42edad0,
        21,
        false,
    );
}

#[test]
fn golden_vector_112() {
    check(
        "322e31333339313035363231333138353439652d3538",
        0x33f56e7fd26c1220,
        22,
        false,
    );
}

#[test]
fn golden_vector_113() {
    check(
        "2d372e32393539363937333535333330313539652d3739",
        0xafb5a09558298d60,
        23,
        false,
    );
}

#[test]
fn golden_vector_114() {
    check(
        "322e32333933353137313533353231323536652d323935",
        0x02c24e7e519a2956,
        23,
        false,
    );
}

#[test]
fn golden_vector_115() {
    check(
        "2d322e33303637363432343633343535323536652b323935",
        0xfd420f24d7b6a301,
        24,
        false,
    );
}

#[test]
fn golden_vector_116() {
    check(
        "322e363433353832373133323139383938652d3835",
        0x2e606f169e5fc772,
        21,
        false,
    );
}

#[test]
fn golden_vector_117() {
    check(
        "322e38373731383939393736383433343631652d313138",
        0x278737b2e5a51628,
        23,
        false,
    );
}

#[test]
fn golden_vector_118() {
    check(
        "2d382e363832343731323438393436353738652b323736",
        0xf96f58db5187dbba,
        23,
        false,
    );
}

#[test]
fn golden_vector_119() {
    check(
        "362e333335303134373938333438313135652b313833",
        0x6617dac04edc1718,
        22,
        false,
    );
}

#[test]
fn golden_vector_120() {
    check(
        "2d352e38323833383230323539373035303233652d313437",
        0xa192a1a48afc758f,
        24,
        false,
    );
}

#[test]
fn golden_vector_121() {
    check(
        "392e3534323139303834303934393533652b323136",
        0x6cfbae29da6966ea,
        21,
        false,
    );
}

#[test]
fn golden_vector_122() {
    check(
        "2d312e393732333537353939353736363835652d333033",
        0x8115a4257c8b5484,
        23,
        false,
    );
}

#[test]
fn golden_vector_123() {
    check(
        "2d312e303830303131313030373432313134652b333035",
        0xff43aface30cda15,
        23,
        false,
    );
}

#[test]
fn golden_vector_124() {
    check(
        "2d312e32333236363536323332343034393234652b3236",
        0xc5597dad6eeed74f,
        23,
        false,
    );
}

#[test]
fn golden_vector_125() {
    check(
        "322e39323432333537393438323539363531652d323838",
        0x043c7f6274a54df9,
        23,
        false,
    );
}

#[test]
fn golden_vector_126() {
    check(
        "2d342e36393033373437373230383530383931652d313938",
        0x96f6706650f6f98c,
        24,
        false,
    );
}

#[test]
fn golden_vector_127() {
    check(
        "2d312e393831303135363331323631343239652d323937",
        0x8254bab031215717,
        23,
        false,
    );
}

#[test]
fn golden_vector_128() {
    check(
        "392e383437333834343330323936393035652b313630",
        0x615c0460b5b5e53a,
        22,
        false,
    );
}

#[test]
fn golden_vector_129() {
    check(
        "2d382e38353938353536333933373232333434652d313631",
        0x9eb3ee0b86b92cd7,
        24,
        false,
    );
}

#[test]
fn golden_vector_130() {
    check(
        "2d362e37333239353237313830313238353439652d3231",
        0xbbbfcba3f024c1ad,
        23,
        false,
    );
}

#[test]
fn golden_vector_131() {
    check(
        "2d312e33313131363133363231353039383839652d313338",
        0xa34f3a5e0a6f6adf,
        24,
        false,
    );
}

#[test]
fn golden_vector_132() {
    check(
        "2d382e333930383439373435343232373931652b3830",
        0xd0bc4e7ab1952033,
        22,
        false,
    );
}

#[test]
fn golden_vector_133() {
    check(
        "312e37343638353035383430393232343133652b323239",
        0x6f870b24e5adc6f2,
        23,
        false,
    );
}

#[test]
fn golden_vector_134() {
    check(
        "2d342e37343732393234303135353137313937652d313730",
        0x9cc6eebfa95b58b4,
        24,
        false,
    );
}

#[test]
fn golden_vector_135() {
    check(
        "352e32333438303439353434373838373531652d3132",
        0x3d9705ddcd159062,
        22,
        false,
    );
}

#[test]
fn golden_vector_136() {
    check(
        "2d332e35343031363138393338373139383037652d3234",
        0xbb111e8216610657,
        23,
        false,
    );
}

#[test]
fn golden_vector_137() {
    check(
        "342e38343639393636383536363339363632652b323036",
        0x6ad827cd91099b90,
        23,
        false,
    );
}

#[test]
fn golden_vector_138() {
    check(
        "2d342e37303937343835343536333438363037652d313130",
        0xa93c50f769eddb2c,
        24,
        false,
    );
}

#[test]
fn golden_vector_139() {
    check(
        "312e363537393338333136343932303838652d323532",
        0x0ba84f7ff9d87d40,
        22,
        false,
    );
}

#[test]
fn golden_vector_140() {
    check(
        "2d332e30383032313932383435393533393338652d313736",
        0x9b7f3462dc3de3e5,
        24,
        false,
    );
}

#[test]
fn golden_vector_141() {
    check(
        "342e33303034313831383238363039323832652b3836",
        0x51ebabba638ea7e3,
        22,
        false,
    );
}

#[test]
fn golden_vector_142() {
    check(
        "392e38313734353332363039313237343634652b3838",
        0x5268acfcfe4cc14e,
        22,
        false,
    );
}

#[test]
fn golden_vector_143() {
    check(
        "2d352e313733373136383836313133303633652b3433",
        0xc9028f4efe95f9ed,
        22,
        false,
    );
}

#[test]
fn golden_vector_144() {
    check(
        "2d342e35373638313839303934353534363931652d313839",
        0x98d4645dba3ca856,
        24,
        false,
    );
}

#[test]
fn golden_vector_145() {
    check(
        "352e383336313633333233333030333736652d323031",
        0x165c97370200c4ca,
        22,
        false,
    );
}

#[test]
fn golden_vector_146() {
    check(
        "372e31373932313439383537363838343735652d313334",
        0x244a1735513f275b,
        23,
        false,
    );
}

#[test]
fn golden_vector_147() {
    check(
        "2d322e343833383030353531303837323934652d323733",
        0x87557facaa3ad4bf,
        23,
        false,
    );
}

#[test]
fn golden_vector_148() {
    check(
        "342e363637313436323830343730323338652d323635",
        0x090e191648e7bd6c,
        22,
        false,
    );
}

#[test]
fn golden_vector_149() {
    check(
        "2d362e303331373333353736343135383231652b3833",
        0xd153df04b235d226,
        22,
        false,
    );
}

#[test]
fn golden_vector_150() {
    check(
        "352e31383530353933333231383436363738652d323838",
        0x044943d8114e44c1,
        23,
        false,
    );
}

#[test]
fn golden_vector_151() {
    check(
        "2d332e313339383635343439303338323938652b323833",
        0xfacb070713f7523e,
        23,
        false,
    );
}

#[test]
fn golden_vector_152() {
    check(
        "312e303137333030383032323031343836652b333030",
        0x7e384e0d6013a8b3,
        22,
        false,
    );
}

#[test]
fn golden_vector_153() {
    check(
        "322e30343033323231313031373933363732652b313234",
        0x59bedd55bc809faf,
        23,
        false,
    );
}

#[test]
fn golden_vector_154() {
    check(
        "2d362e323131303437313038333331343134652b323634",
        0xf6e8a7e0b7d26673,
        23,
        false,
    );
}

#[test]
fn golden_vector_155() {
    check(
        "2d332e32363535323531383738313034333031652d313630",
        0x9ed25d38a1e71238,
        24,
        false,
    );
}

#[test]
fn golden_vector_156() {
    check(
        "332e34353632373433373333313633313738652d323739",
        0x061f5e875f9616cc,
        23,
        false,
    );
}

#[test]
fn golden_vector_157() {
    check(
        "2d372e33333739353837313838383533393835652b3538",
        0xcc27614d4628b487,
        23,
        false,
    );
}

#[test]
fn golden_vector_158() {
    check(
        "352e303230393430343731303639303232652b313532",
        0x5fa32c5e813fe184,
        22,
        false,
    );
}

#[test]
fn golden_vector_159() {
    check(
        "312e35303530323832383432373930323239652b323934",
        0x7d02da20cbc445ee,
        23,
        false,
    );
}

#[test]
fn golden_vector_160() {
    check(
        "2d312e31383635333333393135393330303533652b313030",
        0xd4b5b2f71f706da7,
        24,
        false,
    );
}

#[test]
fn golden_vector_161() {
    check(
        "2d312e37353038373731383330323836353532652d3835",
        0xae55c4cfecbef927,
        23,
        false,
    );
}

#[test]
fn golden_vector_162() {
    check(
        "322e373235373735313131323632353637652b323137",
        0x6d13c47f04d742ef,
        22,
        false,
    );
}

#[test]
fn golden_vector_163() {
    check(
        "312e333431343336363834393833313037652b313034",
        0x558df239e0c08e14,
        22,
        false,
    );
}

#[test]
fn golden_vector_164() {
    check(
        "332e333030343136333433323136343734652d3735",
        0x3077e297fa15c2b8,
        21,
        false,
    );
}

#[test]
fn golden_vector_165() {
    check(
        "2d342e393832363037303138383031353737652d3737",
        0xb01713ee924df0d5,
        22,
        false,
    );
}

#[test]
fn golden_vector_166() {
    check(
        "2d362e32323635313832303531373939343432652d323535",
        0x8b275f72c6dcfacd,
        24,
        false,
    );
}

#[test]
fn golden_vector_167() {
    check(
        "2d382e303736363636353436343836373339652d323033",
        0x95f9529974a8fc0f,
        23,
        false,
    );
}

#[test]
fn golden_vector_168() {
    check(
        "2d322e34353736343030393537333937343532652b313134",
        0xd7afef6a90d659a6,
        24,
        false,
    );
}

#[test]
fn golden_vector_169() {
    check(
        "312e30333339363138363032383839393136652b313139",
        0x58a4804527083680,
        23,
        false,
    );
}

#[test]
fn golden_vector_170() {
    check(
        "322e38303333393733313439303831383136652d323236",
        0x111a90820540db1a,
        23,
        false,
    );
}

#[test]
fn golden_vector_171() {
    check(
        "362e393336333134343038303233323936652b313536",
        0x60802aabf4a7e460,
        22,
        false,
    );
}

#[test]
fn golden_vector_172() {
    check(
        "312e34323430303533363039303033303934652b323132",
        0x6bfb125761645d1e,
        23,
        false,
    );
}

#[test]
fn golden_vector_173() {
    check(
        "382e323936363737333435363534353535652b323539",
        0x75e595948b5f7d1e,
        22,
        false,
    );
}

#[test]
fn golden_vector_174() {
    check(
        "342e373033333638373339363236303131652b313638",
        0x62f3f0b6c093ef09,
        22,
        false,
    );
}

#[test]
fn golden_vector_175() {
    check(
        "2d342e31323030343636303030353935303036652b313738",
        0xe50455a56cd1a051,
        24,
        false,
    );
}

#[test]
fn golden_vector_176() {
    check(
        "312e35323938383239303034383739383035652b313833",
        0x65f70b0845683462,
        23,
        false,
    );
}

#[test]
fn golden_vector_177() {
    check(
        "302e3338353732333436303433303332343739",
        0x3fd8afb173f648d4,
        19,
        false,
    );
}

#[test]
fn golden_vector_178() {
    check(
        "2d372e363638343033363137343635333935652d323836",
        0x84bd3119a195a97f,
        23,
        false,
    );
}

#[test]
fn golden_vector_179() {
    check(
        "382e30313439353833343637383933303731652b313539",
        0x61223e2dce51bb75,
        23,
        false,
    );
}

#[test]
fn golden_vector_180() {
    check(
        "332e323939363037353834323235313438652b323433",
        0x727eed75234c884a,
        22,
        false,
    );
}

#[test]
fn golden_vector_181() {
    check(
        "312e31353334353639393033313033363336652d3037",
        0x3e7ef67eff2cca7d,
        22,
        false,
    );
}

#[test]
fn golden_vector_182() {
    check(
        "362e31323130333434393238353539383334652d323233",
        0x11cc5239f7b01261,
        23,
        false,
    );
}

#[test]
fn golden_vector_183() {
    check(
        "2d342e303734373430333036393535313232652b3137",
        0xc3969e8de35b35e2,
        22,
        false,
    );
}

#[test]
fn golden_vector_184() {
    check(
        "2d332e303634303536353832303330343339652b333031",
        0xfe86e067906fb8e8,
        23,
        false,
    );
}

#[test]
fn golden_vector_185() {
    check(
        "312e30353636303031343736373033333232652d313438",
        0x21359ddd2f12302e,
        23,
        false,
    );
}

#[test]
fn golden_vector_186() {
    check(
        "2d382e323135333135313236343938343133652b313830",
        0xe57fad5d18c74035,
        23,
        false,
    );
}

#[test]
fn golden_vector_187() {
    check(
        "312e373037313431363133323438313635652b333034",
        0x7f18e4da466cf52d,
        22,
        false,
    );
}

#[test]
fn golden_vector_188() {
    check(
        "332e36383337313638383134343830393333652d313630",
        0x1ed4b7459b5b4d3d,
        23,
        false,
    );
}

#[test]
fn golden_vector_189() {
    check(
        "322e303830353534353136333935383938652b3837",
        0x5210bbe8e08ea1c6,
        21,
        false,
    );
}

#[test]
fn golden_vector_190() {
    check(
        "2d322e37313435383530323036313738363436652d3234",
        0xbb0a40fcaa563d1c,
        23,
        false,
    );
}

#[test]
fn golden_vector_191() {
    check(
        "352e30303937323534343339373335333132652b3337",
        0x47c2d830abe42e92,
        22,
        false,
    );
}

#[test]
fn golden_vector_192() {
    check(
        "2d372e36353034343036333336383233313737652b323439",
        0xf3d118bb8014adbe,
        24,
        false,
    );
}

#[test]
fn golden_vector_193() {
    check(
        "342e30373131323530303633323339363233652b3534",
        0x4b4540946236fa63,
        22,
        false,
    );
}

#[test]
fn golden_vector_194() {
    check(
        "312e33303732393937353232333837363735652b313334",
        0x5bc705afd51a45d7,
        23,
        false,
    );
}

#[test]
fn golden_vector_195() {
    check(
        "382e323730363739303437383330313936652d3039",
        0x3e41c2da989f8ec6,
        21,
        false,
    );
}

#[test]
fn golden_vector_196() {
    check(
        "342e303032303130343335303537373637652d3336",
        0x389547411bf9f124,
        21,
        false,
    );
}

#[test]
fn golden_vector_197() {
    check(
        "322e36333230383634363936353034353332652d3532",
        0x353935db7462d527,
        22,
        false,
    );
}

#[test]
fn golden_vector_198() {
    check(
        "312e363439343239393630363434373838652d3933",
        0x2cab8652703e3b09,
        21,
        false,
    );
}

#[test]
fn golden_vector_199() {
    check(
        "342e32353230303132353939353335323934652b313335",
        0x5c1766647f28d44d,
        23,
        false,
    );
}

#[test]
fn golden_vector_200() {
    check(
        "2d312e323636323135373739373236383336652d313838",
        0x98ec354afa311dda,
        23,
        false,
    );
}

#[test]
fn golden_vector_201() {
    check(
        "312e33383337393631343937393731393233652b333033",
        0x7ee024a10e6f24ed,
        23,
        false,
    );
}

#[test]
fn golden_vector_202() {
    check(
        "2d372e31313539323335363236393433333935652b3730",
        0xcea49ee2af6d7a59,
        23,
        false,
    );
}

#[test]
fn golden_vector_203() {
    check(
        "2d312e393839333832373637313930373737652b333037",
        0xffbc546a8f555f07,
        23,
        false,
    );
}

#[test]
fn golden_vector_204() {
    check(
        "2d352e35373035363139303637343031333339652b313537",
        0xe0b03ab70224f895,
        24,
        false,
    );
}

#[test]
fn golden_vector_205() {
    check(
        "2d392e353931353033393134353135373236652d3935",
        0xac699bf1e93e069e,
        22,
        false,
    );
}

#[test]
fn golden_vector_206() {
    check(
        "312e31333033383334363034393333353932652b323035",
        0x6a8206dc9bac0e86,
        23,
        false,
    );
}

#[test]
fn golden_vector_207() {
    check(
        "2d332e32353335373131303036353331373636652d3437",
        0xb647c687004a91ee,
        23,
        false,
    );
}

#[test]
fn golden_vector_208() {
    check(
        "2d322e31373932383835343137323436333738652d323230",
        0x9253b1a2e89a4739,
        24,
        false,
    );
}

#[test]
fn golden_vector_209() {
    check(
        "2d332e34383835333631343334393237333335652d323035",
        0x957c00096bd54e0a,
        24,
        false,
    );
}

#[test]
fn golden_vector_210() {
    check(
        "352e32363136383539363737383832313738652d3931",
        0x2d3126314d2a6707,
        22,
        false,
    );
}

#[test]
fn golden_vector_211() {
    check(
        "2d342e35353932313136333639333833323032652b323936",
        0xfd864ee46ef973c8,
        24,
        false,
    );
}

#[test]
fn golden_vector_212() {
    check(
        "2d312e34393732323134383835333737373132652b313831",
        0xe58cdd8f78dfbf77,
        24,
        false,
    );
}

#[test]
fn golden_vector_213() {
    check(
        "342e31343432353531373634383135363034652b313039",
        0x56b1a564fb4f9877,
        23,
        false,
    );
}

#[test]
fn golden_vector_214() {
    check(
        "392e33383138303236323130313336343639652d313030",
        0x2b606a9190fb9d75,
        23,
        false,
    );
}

#[test]
fn golden_vector_215() {
    check(
        "322e313930393031333835343534343738652d3537",
        0x342b81491b71ada8,
        21,
        false,
    );
}

#[test]
fn golden_vector_216() {
    check(
        "2d372e30303230363637363536393532333438652b313339",
        0xdcf784fe828f74e4,
        24,
        false,
    );
}

#[test]
fn golden_vector_217() {
    check(
        "362e333734333332353332313934333131393733303532396538",
        0x41c2ff3ad29c1652,
        26,
        false,
    );
}

#[test]
fn golden_vector_218() {
    check(
        "332e353631343535363335363435333730313033353036363533393131323534353334353732343965323033",
        0x6a322cc1eb8edca3,
        44,
        false,
    );
}

#[test]
fn golden_vector_219() {
    check(
        "322e373631343137373337343530373232363632383537353732393637333436383138373931393139652d333036",
        0x007f06b0d3faf87d,
        46,
        false,
    );
}

#[test]
fn golden_vector_220() {
    check(
        "372e3335393738313834393538323739393132353337343930363735652d313433",
        0x226cb81fcfa6dad5,
        33,
        false,
    );
}

#[test]
fn golden_vector_221() {
    check(
        "342e33393330323731373936323837333338383033333134323635303337383332652d313836",
        0x19731d4ada37efc9,
        38,
        false,
    );
}

#[test]
fn golden_vector_222() {
    check(
        "332e33313137343738373538353231393530373931353432313935383833313736373834333236652d313035",
        0x2a3e61c7e9b53975,
        44,
        false,
    );
}

#[test]
fn golden_vector_223() {
    check(
        "332e36333431343838373537323236323935343338383531353534353432653430",
        0x485ab31309186b46,
        33,
        false,
    );
}

#[test]
fn golden_vector_224() {
    check(
        "312e383032363935313337323839363632353235353532353832383435373935333135393236653137",
        0x43840391574d2df3,
        41,
        false,
    );
}

#[test]
fn golden_vector_225() {
    check(
        "312e3833393239323034373036333737353132333234333130303138343935393031323465323830",
        0x7a2036597da8e50f,
        40,
        false,
    );
}

#[test]
fn golden_vector_226() {
    check(
        "322e39363032363430393439393036383032323530343435383833333234303333393835353665323632",
        0x766e154d707d7322,
        42,
        false,
    );
}

#[test]
fn golden_vector_227() {
    check("362e3031333937323165313531", 0x5f725f4f30089a89, 13, false);
}

#[test]
fn golden_vector_228() {
    check(
        "312e343034363137373631373235652d3432",
        0x373f52f328990969,
        18,
        false,
    );
}

#[test]
fn golden_vector_229() {
    check(
        "362e33343131393236353533343035323332333737653636",
        0x4dce1b4af04eca5b,
        24,
        false,
    );
}

#[test]
fn golden_vector_230() {
    check(
        "382e3730383434323735393131343835303138363236343531383132343135343137303637343530652d3531",
        0x358a10d3293efcb0,
        44,
        false,
    );
}

#[test]
fn golden_vector_231() {
    check(
        "332e3637333034333030323934323232343031333935313535383230383239652d333033",
        0x01242698d68cfa7b,
        36,
        false,
    );
}

#[test]
fn golden_vector_232() {
    check("3965323834", 0x7b1835a49e91d06e, 5, false);
}

#[test]
fn golden_vector_233() {
    check("382e3131652d323839", 0x041f9d21e894e283, 9, false);
}

#[test]
fn golden_vector_234() {
    check(
        "362e393831313432373932323237313331383435303335353832652d323634",
        0x094c2351c7737343,
        31,
        false,
    );
}

#[test]
fn golden_vector_235() {
    check(
        "392e3734393832383638323035652d3831",
        0x2f527f2c16b1bc1b,
        17,
        false,
    );
}

#[test]
fn golden_vector_236() {
    check(
        "342e32333338303330373030303838373735383433653537",
        0x4be5955eedeb848b,
        24,
        false,
    );
}

#[test]
fn golden_vector_237() {
    check(
        "322e3230393839303737383833393435373435353631323138373265323435",
        0x72e02eb52bd58047,
        31,
        false,
    );
}

#[test]
fn golden_vector_238() {
    check(
        "352e32333738313630393136353637393833313037363930393534313138363237383834652d323339",
        0x0e75d42221bdc02d,
        41,
        false,
    );
}

#[test]
fn golden_vector_239() {
    check(
        "342e303336323334393534353930303236363439653230",
        0x4435e166b6a18b58,
        23,
        false,
    );
}

#[test]
fn golden_vector_240() {
    check("322e34652d3437", 0x364189bba7d8a3e2, 7, false);
}

#[test]
fn golden_vector_241() {
    check(
        "322e303632343334303734323635323133393135313165323235",
        0x6eb649a0243ff486,
        26,
        false,
    );
}

#[test]
fn golden_vector_242() {
    check(
        "392e3239353532343736373833303130363332652d333232",
        0x00000000000000bc,
        24,
        true,
    );
}

#[test]
fn golden_vector_243() {
    check(
        "322e38373332333439373339353531303238353534652d313239",
        0x253fddba716d1aae,
        26,
        false,
    );
}

#[test]
fn golden_vector_244() {
    check("342e3365313434", 0x5df609f75cf5e2d7, 7, false);
}

#[test]
fn golden_vector_245() {
    check(
        "392e30323734373635323037303334353633363338353737343530383036313337313830653137",
        0x43a90e6869c6f117,
        39,
        false,
    );
}

#[test]
fn golden_vector_246() {
    check(
        "382e39373637373538383035393136353037393736323136303432393836303537373332363335652d3831",
        0x2f5107b9cef1070b,
        43,
        false,
    );
}

#[test]
fn golden_vector_247() {
    check(
        "342e303435373330323537343933313437333039383837343934373465333035",
        0x7f626faa83dbaccd,
        32,
        false,
    );
}

#[test]
fn golden_vector_248() {
    check("352e303631303137652d323035", 0x15844f89dc6aa61b, 13, false);
}

#[test]
fn golden_vector_249() {
    check(
        "382e373438333136303532363630363637303035313339353839333539303931323738652d313733",
        0x1c35a322809ebce0,
        40,
        false,
    );
}

#[test]
fn golden_vector_250() {
    check("332e3438653438", 0x4a030c84c6a13b31, 7, false);
}

#[test]
fn golden_vector_251() {
    check(
        "392e3531353933353431652d313337",
        0x23b1b4d497b5ef34,
        15,
        false,
    );
}

#[test]
fn golden_vector_252() {
    check(
        "392e3531323732323439373939303632303430363133383765313037",
        0x5659ec4e0ae0fa5d,
        28,
        false,
    );
}

#[test]
fn golden_vector_253() {
    check(
        "322e3735323836343032383132323038323537313239333539383332652d323233",
        0x11b97968f0ee2a60,
        33,
        false,
    );
}

#[test]
fn golden_vector_254() {
    check("3765323334", 0x70b19ccddc08f73b, 5, false);
}

#[test]
fn golden_vector_255() {
    check(
        "342e35373931363037353932343532383532343131383335323831383130323937343439652d333137",
        0x00000000008d6c65,
        41,
        true,
    );
}

#[test]
fn golden_vector_256() {
    check("34652d313634", 0x1e026d73f9d764b9, 6, false);
}

#[test]
fn golden_vector_257() {
    check(
        "332e3331383632383837333537363434323733652d313432",
        0x22902ff84ea57de1,
        24,
        false,
    );
}

#[test]
fn golden_vector_258() {
    check(
        "332e303636393633393239303765313532",
        0x5f976c6728b308cc,
        17,
        false,
    );
}

#[test]
fn golden_vector_259() {
    check(
        "312e333432343035313631323837333637652d313535",
        0x1fc709d14645551f,
        22,
        false,
    );
}
