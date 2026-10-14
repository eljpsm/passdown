Only the last definition of a label is used[^a][^b]. Earlier ones are removed,
also from inside a quote.

> [^a]: Unused.
>     
>     Still unused.

[^a]: Used.

[^b]: Unused.

[^b]: Used.

Removing one can leave a list item empty[^c]:

- a
  - [^c]: Unused.
  - c

[^c]: Used.
