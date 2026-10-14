A wide ordered marker narrows to one space, so indented code after the list
needs a separator:

1.   item

    code after the list

The same after a bullet list:

-    item

    code after the list

In a quote:

> 1.   item
>
>     quoted code after the list

In a footnote[^1].

Nested in an item:

- outer

  1.   inner

      code after the nested list

An untyped fence that keeps its offset:

1.   item

   ```
   x
   ```

An indented HTML block:

1.   item

   <div>
   html after the list
   </div>

In a tight item:

- a
  1.   b
     <div>
     x
     </div>
- c

Two indented blocks that a removed link definition kept apart:

    first block

[ref]: /url

    second block

In a quote:

>     first quoted
>
> [ref2]: /url
>
>     second quoted

In an item:

- item

      first in item

  [ref3]: /url

      second in item

[^1]: Footnote

    1.   item

        code in the footnote
