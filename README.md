# Moodle CLI

Moodle を CUI から操作するためのツールです。コース内の小テスト・リソース・課題をターミナル上で操作できます。

## 機能

- ログイン（Shibboleth / TOTP 2FA 対応）
- コース一覧 / セクション / 活動一覧の閲覧
- 小テストの受験
- レビュークイズの表示
- PDF / HTML リソースのダウンロード
- 課題ファイルのアップロード

## 使い方

```text
moodle_cli [username] [password]
```

ユーザー名を省略するとプロンプトが表示されます。

TOTP トークンは環境変数 `UEC_KEY` から読み込むか、入力を促されます。

## ライセンス

MIT License

Copyright (c) 2026 inorganic

## プルリクエスト歓迎

バグ報告・機能追加・改善など、どんな PR でも歓迎します。
