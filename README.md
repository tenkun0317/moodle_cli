# Moodle CLI

Moodle を CUI から操作するためのツールです。コース内の小テスト・リソース・課題をターミナル上で操作できます。

## 機能

- ログイン（Shibboleth / TOTP・Mail OTP 2FA 対応）
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

TOTPは、環境変数 `UEC_KEY` にあるトークンから自動生成するか、入力を促されます。Mail OTP は手動でコードを入力します（TOTP と Mail OTP の両方が利用可能な場合は TOTP が優先されます）。

## ライセンス

MIT License

Copyright (c) 2026 inorganic

## プルリクエスト歓迎

バグ報告・機能追加・改善など、どんな PR でも歓迎します。
