A tilde fence becomes a backtick fence:

~~~~ rust
fn main() {
    println!("hi");
}
~~~~

A fence whose content contains backticks gets a longer fence:

`````` text
```
inner backticks
``````

A backtick run indented up to three spaces could still close the fence, so it
grows the fence too:

~~~~ text
   ```
content
~~~~

Code in a list item:

- item with code

    ```python
    def f():
        return 1
    ```

Code in a quote:

> ```sh
> echo hello
> ```

Extra info string metadata is preserved:

``` rust,no_run
let x = 1;
```
