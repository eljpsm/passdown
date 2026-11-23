Only the first definition of a label is used[^a][^b]. Later ones are removed,
also from inside a quote.

[^a]: Used.

> [^a]: Unused.
>     
>     Still unused.

[^b]: Used.

[^b]: Unused.

Removing one can leave a list item empty[^c]:

[^c]: Used.

- a
  - [^c]: Unused.
  - c
