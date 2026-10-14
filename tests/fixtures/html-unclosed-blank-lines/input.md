An unclosed HTML block keeps the blank lines it absorbed:

- <pre>
  a
   
- b

- <pre>
  a


- c

Through nested items, quotes, and footnotes[^1][^2]:

- - <pre>
    a


- d

> <pre>
> a
>
>

[^1]: <pre>
    a


[^2]: b
