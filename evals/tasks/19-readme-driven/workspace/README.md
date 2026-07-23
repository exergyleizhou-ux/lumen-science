# greeter

## API

- `Hello(name string) string` — returns `"Hello, <name>!"`
- `Goodbye(name string) string` — returns `"Goodbye, <name>!"`
- `IsEmpty(s string) bool` — returns true when s is "" or whitespace-only

All functions must handle empty strings gracefully: Hello("") returns "Hello, !" and Goodbye("") returns "Goodbye, !".
