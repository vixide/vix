# org-capture task template

```txt
* TODO %^{Task}
  %^{Priority|0|0|1|2|3|4|5|6|7|8|9}
  %^g
  SCHEDULED: %^{Scheduled}t DEADLINE: %^{Deadline}
  :properties:
  :contact: %^{contact}
  :created: %U
  :effort: %^{effort|0:10|0:20|0:30|1:00|2:00|3:00}
  :category: %^{category|inbox|work|home|out}
  :end:
  :notes:
  %^{notes}
  :end:
  %a
```
