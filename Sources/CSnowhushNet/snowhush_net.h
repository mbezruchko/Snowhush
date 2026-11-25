// Rooms for a snowball fight between friends: net/src/lib.rs.
#ifndef SNOWHUSH_NET_H
#define SNOWHUSH_NET_H

void sn_host(const char *name, const char *relay);
void sn_join(const char *ticket, const char *name, const char *relay);
void sn_throw(const char *at);
char *sn_next(void);
void sn_leave(void);
void sn_leave_now(void);
void sn_free(char *text);

#endif
