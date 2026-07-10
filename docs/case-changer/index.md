# Case Changer

The case changer transforms the editor's current text selection into a
different letter-case style. Each transform is a menu item under **Edit →
Case**; choosing one rewrites whatever is selected.

## How to use

1. Select the text you want to transform.
2. Open **Edit → Case**.
3. Choose the target style.

The transform applies to the editor text selection. With nothing selected
there is no text to convert.

## Available transforms

The **Case** submenu lists seven styles. Each label shows the result of
applying it to the example words `foo bar`:

| Menu item        | Result    |
| ---------------- | --------- |
| Upper (FOO BAR)  | `FOO BAR` |
| Lower (foo bar)  | `foo bar` |
| Title (Foo Bar)  | `Foo Bar` |
| Kebab (foo-bar)  | `foo-bar` |
| Snake (foo_bar)  | `foo_bar` |
| Camel (fooBar)   | `fooBar`  |
| Pascal (FooBar)  | `FooBar`  |

- **Upper** — every letter uppercase.
- **Lower** — every letter lowercase.
- **Title** — the first letter of each word uppercase.
- **Kebab** — words joined with hyphens.
- **Snake** — words joined with underscores.
- **Camel** — words joined with no separator; first word lowercase, each
  following word capitalized.
- **Pascal** — words joined with no separator; every word capitalized.

## Examples

Select `hello world` and choose:

- **Upper** → `HELLO WORLD`
- **Title** → `Hello World`
- **Kebab** → `hello-world`
- **Snake** → `hello_world`
- **Camel** → `helloWorld`
- **Pascal** → `HelloWorld`

---

Vix™ and Vix IDE™ are trademarks.
