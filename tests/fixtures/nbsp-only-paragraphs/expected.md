A paragraph left empty by punctuation normalization keeps its characters as
references:

&nbsp;

&nbsp;&#x20;&#x2003;&#xA;&#x200B;

&#x20;

Between hard breaks:

&nbsp;\
&nbsp;

In list items, a quote and a task item:

- &nbsp;
- b

1. &nbsp;
2. b

> &nbsp;

- [ ] &nbsp;
- [x] b

Prose still converts them: here.

Multi-line:

&nbsp;&#xA;&#x20;

Inside inline markup: *&nbsp;*, **&nbsp;**, ~~&nbsp;~~, ***&nbsp;***,
[&nbsp;](https://example.com) and ![&nbsp;](a.png).

A line after a hard break:

foo\
&nbsp;

a\
&nbsp;\
b

# &nbsp;

| a      | b   |
| ------ | --- |
| &nbsp; | x   |
