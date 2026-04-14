<p align="center">
  <picture>
    <img src="https://raw.githubusercontent.com/JunJ-M/Talkiwi/main/assets/kiwi-sun.png" alt="Talkiwi" width="380">
  </picture>
</p>

<h1 align="center">Talkiwi</h1>

<p align="center">
  <strong>AIワークフロー向けオープンソース マルチトラック音声コンテキストコンパイラー</strong><br/>
  <sub>音声 + 操作 + テキスト選択 + スクリーンショットをAI対応の構造化プロンプトに変換</sub>
</p>

<p align="center">
  <!-- Build -->
  <a href="https://github.com/JunJ-M/Talkiwi/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/JunJ-M/Talkiwi/ci.yml?branch=main&label=CI&style=flat-square&logo=github" alt="CI Status">
  </a>
  <!-- License -->
  <a href="./LICENSE">
    <img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue?style=flat-square" alt="ライセンス">
  </a>
  <!-- Version -->
  <a href="https://github.com/JunJ-M/Talkiwi/releases">
    <img src="https://img.shields.io/github/v/release/JunJ-M/Talkiwi?style=flat-square&color=orange&label=release" alt="最新リリース">
  </a>
  <!-- Platform -->
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey?style=flat-square&logo=apple" alt="プラットフォーム">
  <!-- Rust -->
  <img src="https://img.shields.io/badge/rust-1.78%2B-orange?style=flat-square&logo=rust" alt="Rustバージョン">
  <!-- Stars -->
  <a href="https://github.com/JunJ-M/Talkiwi/stargazers">
    <img src="https://img.shields.io/github/stars/JunJ-M/Talkiwi?style=flat-square&color=yellow" alt="Stars">
  </a>
  <!-- PRs welcome -->
  <a href="https://github.com/JunJ-M/Talkiwi/pulls">
    <img src="https://img.shields.io/badge/PRs-welcome-brightgreen?style=flat-square" alt="PRs歓迎">
  </a>
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./README.zh-CN.md">简体中文</a> ·
  <a href="./README.ja.md">日本語</a>
</p>

---

## Talkiwi とは？

**Talkiwi** は音声テキスト変換ツールではありません。**音声コンテキストコンパイラー** です。macOS のデスクトップサイドパネルとして常駐し、あなたが _話している内容_ と _実行している操作_ を同時に記録し、両者を組み合わせてAI対応の構造化Markdownドキュメントを生成します。これをお好みのLLMに貼り付けるだけで使えます。

> **重要な違い：** Talkiwi は何もAIに **送信しません**。  
> 構造化されたプロンプトドキュメントを生成するだけです。どこに送るかはあなたが決めます。

### 解決する課題

| ギャップ                       | 例                                                                                        |
| ------------------------------ | ----------------------------------------------------------------------------------------- |
| コンテキストのない音声         | 「これを修正して」と言っても、モデルは「これ」が何かわからない                            |
| 操作証跡のない文字起こし       | コードを選択し、スクリーンショットを撮り、Issueを開いたが、何もモデルに届かない           |
| 再構成のない生の口語           | 人間の発話にはフィラー、代名詞の飛躍、言い直しが含まれており、LLMに直接入力するには不適切 |
| クローズドなコンテキストモデル | コーディング、ライティング、リサーチでは全く異なるコンテキストが必要                      |

### 出力例

```markdown
## タスク
選択した関数にキャッシュを追加する。エラーがリトライロジックに関連しているか調査する。

## ユーザーの意図
コード修正 + バグ調査

## コンテキスト
### 選択したコード
[選択範囲の関数ソース]

### エラースクリーンショット
[OCRテキスト付きスクリーンショット]

### 参照Issue
[Issue URL + タイトル]

### 環境情報
- リポジトリ: my-project
- ファイル: src/utils/fetcher.ts:42

## 期待する出力
1. エラーの根本原因分析
2. キャッシュ実装の提案
3. コードパッチ
```

---

## 機能一覧（V1 Alpha）

| #   | 機能                                                                | トラック         |
| --- | ------------------------------------------------------------------- | ---------------- |
| 1   | ウィジェットボタンでキャプチャ開始/停止                             | コア             |
| 2   | ローカルASR（whisper.cpp / mlx-whisper）                            | 音声             |
| 3   | クラウドASRオプション（Deepgram / OpenAI Whisper API）              | 音声             |
| 4   | 選択テキストのインジェクション                                      | アーティファクト |
| 5   | アプリ内スクリーンショットツール（範囲選択対応）                    | アーティファクト |
| 6   | 現在のURL + ページタイトルのインジェクション                        | アーティファクト |
| 7   | クリップボード内容のインジェクション                                | アーティファクト |
| 8   | ファイルドラッグイン                                                | アーティファクト |
| 9   | インテントコンパイラー（デフォルトはローカルLLM、クラウドも選択可） | コア             |
| 10  | **自動代名詞解決**（「これ」/「あれ」 → 最近のアーティファクト）    | コア             |
| 11  | 構造化Markdown出力生成                                              | コア             |
| 12  | 折りたたみ可能な常駐サイドバーパネル                                | UI               |
| 13  | マルチトラックタイムラインビューワー                                | UI               |
| 14  | ワンクリックでクリップボードにコピー                                | UI               |
| 15  | セッションをローカルファイルに自動保存                              | ストレージ       |
| 16  | セッション履歴ブラウザー                                            | ストレージ       |
| 17  | Providerの設定（ローカル ↔ クラウド切り替え）                       | 設定             |

---

## アーキテクチャ

Talkiwi は **Tauri 2.0** で構築されており、RustバックエンドとReactフロントエンドを採用しています。

### 技術スタック

| レイヤー               | 選択                                     | 理由                                                       |
| ---------------------- | ---------------------------------------- | ---------------------------------------------------------- |
| アプリシェル           | **Tauri 2.0**（Rust + WebView）          | 約10MBのバンドル、macOSネイティブAPI、優れたパフォーマンス |
| フロントエンド         | React + TypeScript                       | 高速な開発サイクル、豊富なエコシステム                     |
| ASR                    | whisper.cpp / mlx-whisper                | ローカルファースト、Apple Silicon最適化                    |
| インテントコンパイラー | Ollama + 小型モデル（Qwen2.5-7B、Phi-3） | ローカルファースト、Provider切り替え可能                   |
| ストレージ             | SQLite（rusqlite）                       | セッション履歴とイベント保存                               |
| IPC                    | Tauriコマンド + イベントシステム         | Rust ↔ フロントエンド通信                                  |

---

## クイックスタート

### 必要な環境

- macOS 13 Ventura 以降
- Rust 1.78+（`rustup install stable`）
- Node.js 20+ と npm 10+
- [Ollama](https://ollama.ai/)（ローカルインテントコンパイル用）

### インストール方法

```bash
# 1. リポジトリをクローン
git clone https://github.com/JunJ-M/Talkiwi.git
cd Talkiwi

# 2. フロントエンド依存関係をインストール
npm ci --prefix apps/desktop

# 3. ローカルモデルをプル
ollama pull qwen2.5:7b

# 4. 開発モードで起動
npm --prefix apps/desktop run tauri -- dev
```

### リリースDMGのビルド

```bash
npm --prefix apps/desktop run tauri -- build
# 出力先: apps/desktop/src-tauri/target/release/bundle/dmg/
```

---

## プライバシー

Talkiwi は設計上 **ローカルファースト**：

- デフォルトではすべての処理がローカルで実行されます — クラウドProviderを明示的に有効にしない限り、データは端末の外に出ません
- トラックごとの細かい権限付与（スクリーンショット、クリップボード、アクセシビリティ）
- クラウドProviderの使用には明示的な同意が必要
- セッションレベルの **「録音しない」** モード
- オプトインなしのテレメトリーなし

---

## ライセンス

Talkiwi は **MIT** と **Apache 2.0** のデュアルライセンスです。どちらのライセンスでも使用できます。

詳細は [LICENSE](./LICENSE) を参照してください。

---

<p align="center">Talkiwi チームが ☕ を飲みながら製作 · <a href="https://github.com/JunJ-M/Talkiwi/issues">Issueを報告する</a></p>
