- Item:

      line one
      line two
      line three

- Second item

1. First:

       one
       two

2. Second

- Outer

  - Inner:

        nested one
        nested two

  - Inner two

- Two blocks:

      first a
      first b

  Between.

      second a

      second b

- Last

>     quoted one
>     quoted two
>
> After the quote block.

> [!NOTE]
>
>     alert one
>     alert two
>
> After the alert block.

> - Quoted list:
>
>       quoted list one
>       quoted list two
>
> - Quoted next

- Fenced in a list:

  ```
  fenced one
  fenced two
  ```

- After fenced

> ~~~~
> quoted tilde
> ~~~~
>
> After the quoted fence.

Fence offsets keep content that sits left of the fence:

   ```
   a
    b
  c
d
   ```

   ```
    ```
   ```

- Unclosed:

  ```
  x
  y

- Next

Text[^1].

[^1]: Footnote:

        foot one
        foot two

    After the footnote block.

From the phx.new AGENTS.md:

- Elixir lists **do not support index based access via the access syntax**

  **Never do this (invalid)**:

      i = 0
      mylist = ["blue", "green"]
      mylist[i]

  Instead, **always** use `Enum.at`, pattern matching, or `List` for index based list access, ie:

      i = 0
      mylist = ["blue", "green"]
      Enum.at(mylist, i)
