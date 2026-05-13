//! One-shot helper: `cargo run -p fractal-wallet --example dump_quote_body_borsh`
//! prints hex(borsh(QuoteBody)) for the sample used by `tools/provider-http-sample`.

use fractal_wallet::market::QuoteBody;

fn main() {
    let body = QuoteBody {
        quote_id: [0x11u8; 32],
        intent_id: [0xaau8; 32],
        provider_id: [0x22u8; 32],
        price: 1u128,
        expiry_ms: 9_999_999_999_999u64,
    };
    let v = borsh::to_vec(&body).expect("encode");
    println!("len={}", v.len());
    println!(
        "{}",
        v.iter().map(|b| format!("{:02x}", b)).collect::<String>()
    );
}
