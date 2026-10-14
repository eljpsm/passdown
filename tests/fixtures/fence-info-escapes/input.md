A language keeps its escapes and references, so it reads the same next time:

``` a\\*
x
```

``` a&amp;amp;b
x
```

Non-ASCII whitespace stays part of it:

``` rust 
x
```

A plain one is unchanged:

```rust
x
```
