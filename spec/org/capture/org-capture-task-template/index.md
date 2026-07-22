# org-capture task template

```elisp
"* TODO %^{Task}"                                  ; headline, prompts for title
" %^{Priority|0|0|1|2|3|4|5|6|7|8|9}"              ; optional #A/#B/#C via [#…] below
" %^g\n"                                           ; tags (completes against existing)
"SCHEDULED: %^{Scheduled}t DEADLINE: %^{Deadline}t\n"
":properties:\n"
":created:  %U\n"                                  ; inactive timestamp of capture
":effort:   %^{effort|0:10|0:20|0:30|1:00|2:00|3:00}\n" ; estimate for column view/agenda
":category: %^{category|inbox|work|home|out}\n"
":end:\n"
"%a\n"
```
