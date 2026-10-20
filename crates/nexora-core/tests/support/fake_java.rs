//! 結合テスト用の「偽の Java」。Mojang の runtime の `bin/java(.exe)` として配る。
//! Windows では `.exe` の名前でスクリプトを実行できないため、小さい実行ファイルとして作る。
//!
//! processor として呼ばれたとき（`-cp <cp> <Main-Class> 記録先 制御 出力 入力...`）：
//! 記録先に 1 行を足し、制御のファイルの中身（無ければ `ok`）に従って動く。
//! - `ok`：入力がすべてあることを確かめ（無ければ終了コード 5）、出力に書く
//! - `slow`：少し待ってから `ok` と同じ
//! - `fail`：標準エラーに書いて終了コード 3
//! - `spin`：止められるまで動き続ける
//!
//! それ以外（ゲームとしての起動）は `fake game <引数>` を出して終了コード 0。

use std::io::Write;
use std::path::Path;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 6 || args[0] != "-cp" {
        println!("fake game {}", args.join(" "));
        return;
    }
    let (calls, control, output, inputs) = (&args[3], &args[4], &args[5], &args[6..]);
    let mut record = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(calls)
        .expect("open the call record");
    writeln!(record, "{}", args[2]).expect("write the call record");
    drop(record);

    let mode = std::fs::read_to_string(control).unwrap_or_else(|_| "ok".into());
    match mode.trim() {
        "fail" => {
            eprintln!("fake processor failed on purpose");
            std::process::exit(3);
        }
        "spin" => loop {
            std::thread::sleep(Duration::from_millis(50));
        },
        "slow" => std::thread::sleep(Duration::from_millis(300)),
        _ => {}
    }
    for input in inputs {
        if !Path::new(input).is_file() {
            eprintln!("missing input {input}");
            std::process::exit(5);
        }
    }
    let output = Path::new(output);
    if let Some(dir) = output.parent() {
        std::fs::create_dir_all(dir).expect("create the output directory");
    }
    std::fs::write(output, b"patched").expect("write the output");
}
