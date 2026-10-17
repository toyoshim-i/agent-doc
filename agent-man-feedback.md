# agent-man への機能追加・改修依頼: オブジェクト型パラメータおよび動的スキーマのサポート

## 1. 概要と背景

`agent-man` (v0.5.0) を用いて構築された MCP サーバー（例: `karakuri-mcp`）において、MCP クライアント（OpenCode 等、内部で Vercel AI SDK や Zod / Standard Schema を採用しているツール）からツール呼び出しを行う際、**スキーマに未定義の引数（ペイロード）がクライアント側で自動的に削ぎ落とされ (strip)、サーバーに届かない** という問題が発生しています。

### 具体的な事象例（Karakuri の `operate` ツール）
- Karakuri では、汎用操作呼び出しツール `operate` が引数として `{ "operation": "<名前>", "with": { ... } }` を受け取る仕様となっています。
- しかし、現在の `agent-man` の `ToolBuilder` では `with`（可変の JSON オブジェクト）をスキーマとして定義できないため、パラメータには `"operation"` のみを登録しています。
- この時、`agent-man` が生成する `tools/list` の JSON Schema は以下のようになります：
  ```json
  {
    "type": "object",
    "properties": {
      "operation": {
        "type": "string",
        "enum": [ ... ]
      }
    },
    "required": ["operation"],
    "additionalProperties": false
  }
  ```
- このスキーマを受け取った OpenCode は、LLM が `{ "operation": "Nudge the latency offset", "with": { "ms": 5 } }` を出力しても、**`additionalProperties: false` かつ未定義の `with` フィールドを strip してしまい、サーバーには `{"operation": "..."}` のみを送信** します。
- サーバー側（Karakuri）では `with` が空オブジェクト `{}` になるため、「`with.ms is required and is a number`」というエラーを返し続けます。
- クライアント側エージェントからは「どの形式で渡しても `with.<field> is required` が返る（バインディングで引数が渡らない）」と見えてしまいます。

また、`write_procedure` のように `nodes: [{layer, index, source}, ...]` といったバッチ更新用の配列/オブジェクト構造を受け取るツールでも、同様にクライアント側でのスキーマ不整合の原因となっています。

---

## 2. 現在の実装における制約

1. **`ParamType` に複合型（`Object` や `Any`）が存在しない**
   - [`src/mcp/tool.rs`](file:///Users/toyoshim/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/agent-man-0.5.0/src/mcp/tool.rs#L6-L13) の `ParamType` は以下のプリミティブのみに対応しています：
     - `String`, `Integer`, `Number`, `Boolean`, `Enum(Vec<String>)`, `StringArray`
   - 任意のプロパティを持つオブジェクト（`Object`）や、ネストしたオブジェクト配列などを表現できません。

2. **`additionalProperties: false` が固定**
   - [`src/mcp/tool.rs` の `Tool::input_schema()`](file:///Users/toyoshim/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/agent-man-0.5.0/src/mcp/tool.rs#L126-L131) では、スキーマのルートに一律で `"additionalProperties": false` が出力されます。
   - `skip_validation(true)` は `agent-man` サーバー側のバリデーションをスキップするだけであり、公開される JSON Schema 自体は `"additionalProperties": false` のまま変わらないため、クライアント側のスキーマ検証・サニタイズでプロパティが削られます。

3. **カスタム inputSchema を直接指定する手段がない**
   - 複雑なスキーマ（`oneOf`, `anyOf`, ネストした `object` など）を直接 JSON Schema として定義して差し替える手段が `ToolBuilder` にありません。

---

## 3. 改修の提案内容

以下のいずれか、または組み合わせでの機能追加・改修を提案します。

### 提案 A: `ParamType::Object` および `ParamType::Any` の追加（推奨）

`ParamType` にオブジェクト型・任意型を追加し、スキーマ定義と簡易バリデーションを可能にします。

```rust
pub enum ParamType {
    String,
    Integer { min: Option<i64>, max: Option<i64> },
    Number,
    Boolean,
    Enum(Vec<String>),
    StringArray,
    /// 任意の JSON オブジェクト (type: "object")
    Object,
    /// 任意の JSON 値
    Any,
}
```

- **`Tool::input_schema()` での出力**:
  - `ParamType::Object` の場合:
    ```json
    { "type": "object" }
    ```
    （または必要に応じて `additionalProperties: true` を含める）
  - `ParamType::Any` の場合: 空のスキーマ `{}` または型制限なし

- **`Tool::validate_args()` での検証**:
  - `Object` の場合は `val.is_object()` をチェック。
  - `Any` の場合は常に `true`。

### 提案 B: `additionalProperties` の制御オプションの追加

`ToolBuilder` に `additional_properties(bool)` を追加し、ツール単位で追加フィールドを許容できるようにします。

```rust
impl ToolBuilder {
    pub fn additional_properties(mut self, allow: bool) -> Self;
}
```

- `allow` が `true` の場合、生成される `inputSchema` の `"additionalProperties"` を `true`（あるいは省略）にします。
- これにより、`operate` のように動的な追加プロパティを受け取るツールで、クライアント側バリデータによるプロパティの削ぎ落としを防止できます。

### 提案 C: カスタム `input_schema` のオーバーライド指定

手動で構築した JSON Schema（`serde_json::Value`）をそのまま公開できるビルダーメソッドを追加します。

```rust
impl ToolBuilder {
    pub fn input_schema(mut self, schema: serde_json::Value) -> Self;
}
```

- この指定がある場合は、`params` から自動生成する代わりに指定された `schema` をそのまま `to_mcp_definition()` で使用します。
- `operate` ツールの `karakuri://operations` カリキュラムから生成された完全な JSON Schema や、`write_procedure` の `nodes` 配列スキーマなどを完全に公開できるようになります。

---

## 4. 期待される効果と Before / After

### `operate` ツール定義（Karakuri 側での利用例）
```rust
ToolBuilder::new("operate", "Execute an instrument operation ...")
    .param("operation", ParamType::Enum(operable_titles), "heading of the operation", true)
    .param("with", ParamType::Object, "payload object for the operation", false)
    .additional_properties(true)
    .skip_validation(true)
    .handler(...)
    .build()
```

### 公開される MCP `tools/list` スキーマの比較
- **Before**:
  ```json
  {
    "name": "operate",
    "inputSchema": {
      "type": "object",
      "properties": {
        "operation": { "type": "string", "enum": [...] }
      },
      "required": ["operation"],
      "additionalProperties": false
    }
  }
  ```
  *(クライアントが `with` を未知フィールドとして破棄してしまう)*

- **After**:
  ```json
  {
    "name": "operate",
    "inputSchema": {
      "type": "object",
      "properties": {
        "operation": { "type": "string", "enum": [...] },
        "with": {
          "type": "object",
          "description": "payload object for the operation"
        }
      },
      "required": ["operation"],
      "additionalProperties": true
    }
  }
  ```
  *(OpenCode などのクライアント経由でも `with` が保持され、サーバーへ透過する)*
