/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * P1c locale/names compile probe: every one of the 51 closed rows must be declared by its
 * header under the POSIX/XSI feature set the POSIX gate uses. `catan`/`catanh` are math rows
 * (the census assigns them to `m`/`complex.h`), not part of this probe.
 */

#include <nl_types.h>
#include <syslog.h>
#include <search.h>
#include <uchar.h>
#include <netdb.h>
#include <net/if.h>
#include <grp.h>
#include <iconv.h>
#include <libgen.h>
#include <monetary.h>
#include <pwd.h>
#include <regex.h>
#include <ftw.h>
#include <fmtmsg.h>
#include <inttypes.h>
#include <utmpx.h>

static void *const p1c_probes[] = {
    /* nl_types.h */
    (void *)&catopen,
    (void *)&catgets,
    (void *)&catclose,
    /* syslog.h */
    (void *)&openlog,
    (void *)&closelog,
    (void *)&setlogmask,
    (void *)&syslog,
    (void *)&vsyslog,
    /* search.h */
    (void *)&hcreate,
    (void *)&hcreate_r,
    (void *)&hdestroy,
    (void *)&hdestroy_r,
    (void *)&hsearch_r,
    /* uchar.h */
    (void *)&c16rtomb,
    (void *)&mbrtoc16,
    (void *)&c32rtomb,
    (void *)&mbrtoc32,
    (void *)&c8rtomb,
    (void *)&mbrtoc8,
    /* net/if.h */
    (void *)&if_nametoindex,
    (void *)&if_indextoname,
    (void *)&if_nameindex,
    (void *)&if_freenameindex,
    /* grp.h */
    (void *)&putgrent,
    (void *)&getgrouplist,
    (void *)&setgroups,
    (void *)&initgroups,
    (void *)&setgroupent,
    /* iconv.h */
    (void *)&iconv_open,
    (void *)&iconv,
    (void *)&iconv_close,
    /* libgen.h */
    (void *)&dirname,
    (void *)&basename,
    /* monetary.h */
    (void *)&strfmon,
    (void *)&strfmon_l,
    /* pwd.h */
    (void *)&putpwent,
    (void *)&setpassent,
    /* regex.h */
    (void *)&regnsub,
    (void *)&regasub,
    /* ftw.h */
    (void *)&ftw64,
    (void *)&nftw64,
    /* fmtmsg.h */
    (void *)&fmtmsg,
    /* inttypes.h */
    (void *)&umaxabs,
    /* utmpx.h */
    (void *)&updwtmpx,
    (void *)&utmpxname,
    /* netdb.h */
    (void *)&gethostbyname_r,
    (void *)&gethostbyname2_r,
    (void *)&gethostbyaddr_r,
    (void *)&getservbyname_r,
    (void *)&getservbyport_r,
    (void *)&herror,
};

int main(void)
{
    return p1c_probes[0] == 0;
}
