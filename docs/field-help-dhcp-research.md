# DHCP 字段静态证据（RN02 1.0.43）

范围：只读取 `/Volumes/RN02_STATIC/rootfs` 的服务程序、脚本及已有反编译 Lua。没有读取或复制私有配置值，没有 SSH、探测、重启或实时验证。

## odhcpd ELF 名称/类型表

- 原始文件：`usr/sbin/odhcpd`。
- SHA-256：`afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`。
- 文件大小：`70173` 字节。
- ELF32、little-endian、ARM；无节头。名称通过 NUL 结尾 ASCII 串定位；表项按两个 little-endian uint32 读取（字符串虚拟地址、原始类型数）。
- 这是名称与原始类型表证据，不单凭此表推断缺省值、范围、单位或实际通告行为。相邻 RA flag 表的数值也按原始位值列出。

| 字段/符号 | 表项文件偏移 | 字符串文件偏移 | 原始类型/数值 |
| --- | --- | --- | --- |
| `interface` | `0x10990` | `0x1063a` | `3` |
| `ifname` | `0x10998` | `0xfba3` | `3` |
| `networkid` | `0x109a0` | `0xfbaa` | `3` |
| `dynamicdhcp` | `0x109a8` | `0xfbb4` | `7` |
| `leasetime` | `0x109b0` | `0xfca9` | `3` |
| `limit` | `0x109b8` | `0xfd0d` | `5` |
| `start` | `0x109c0` | `0xfbc0` | `5` |
| `master` | `0x109c8` | `0xfbc6` | `7` |
| `upstream` | `0x109d0` | `0xfbcd` | `1` |
| `ra` | `0x109d8` | `0xfa4c` | `3` |
| `dhcpv4` | `0x109e0` | `0xfa7d` | `3` |
| `dhcpv6` | `0x109e8` | `0xfab0` | `3` |
| `ndp` | `0x109f0` | `0x103c3` | `3` |
| `router` | `0x109f8` | `0xfc72` | `1` |
| `dns` | `0x10a00` | `0xfcbd` | `1` |
| `dns_service` | `0x10a08` | `0xfbd6` | `7` |
| `domain` | `0x10a10` | `0xfab7` | `1` |
| `filter_class` | `0x10a18` | `0xfbe2` | `3` |
| `dhcpv4_forcereconf` | `0x10a20` | `0xfbef` | `7` |
| `dhcpv6_raw` | `0x10a28` | `0xfc02` | `3` |
| `dhcpv6_assignall` | `0x10a30` | `0xfc0d` | `7` |
| `dhcpv6_pd` | `0x10a38` | `0xfc1e` | `7` |
| `dhcpv6_na` | `0x10a40` | `0xfc28` | `7` |
| `dhcpv6_hostidlength` | `0x10a48` | `0xfabe` | `5` |
| `ra_default` | `0x10a50` | `0xfc32` | `5` |
| `ra_management` | `0x10a58` | `0xfc3d` | `5` |
| `ra_flags` | `0x10a60` | `0xfad2` | `1` |
| `ra_slaac` | `0x10a68` | `0xfc4b` | `7` |
| `ra_offlink` | `0x10a70` | `0xfc54` | `7` |
| `ra_no_prefix` | `0x10a78` | `0xfc5f` | `7` |
| `ra_preference` | `0x10a80` | `0xfb17` | `3` |
| `ra_advrouter` | `0x10a88` | `0xfc6c` | `7` |
| `ra_mininterval` | `0x10a90` | `0xfc79` | `5` |
| `ra_maxinterval` | `0x10a98` | `0xfc88` | `5` |
| `ra_lifetime` | `0x10aa0` | `0xfc97` | `5` |
| `ra_useleasetime` | `0x10aa8` | `0xfca3` | `7` |
| `ra_reachabletime` | `0x10ab0` | `0xfadb` | `5` |
| `ra_retranstime` | `0x10ab8` | `0xfaec` | `5` |
| `ra_hoplimit` | `0x10ac0` | `0xfafb` | `5` |
| `ra_mtu` | `0x10ac8` | `0xfcb3` | `5` |
| `ra_dns` | `0x10ad0` | `0xfcba` | `7` |
| `pd_manager` | `0x10ad8` | `0xfcc1` | `3` |
| `pd_cer` | `0x10ae0` | `0xfb25` | `3` |
| `ndproxy_routing` | `0x10ae8` | `0xfccc` | `7` |
| `ndproxy_slave` | `0x10af0` | `0xfcdc` | `7` |
| `prefix_filter` | `0x10af8` | `0xfcea` | `3` |
| `preferred_lifetime` | `0x10b00` | `0xfa39` | `3` |
| `legacy` | `0x10b30` | `0xfb60` | `7` |
| `maindhcp` | `0x10b38` | `0xfb67` | `7` |
| `leasefile` | `0x10b40` | `0xfb70` | `3` |
| `leasetrigger` | `0x10b48` | `0xfb7a` | `3` |
| `loglevel` | `0x10b50` | `0xfb87` | `5` |
| `managed-config` | `0x10b08` | `0xfb34` | 位值 `128` |
| `other-config` | `0x10b10` | `0xfb43` | 位值 `64` |
| `home-agent` | `0x10b18` | `0xfb50` | 位值 `32` |
| `none` | `0x10b20` | `0xfb5b` | 位值 `0` |

## dnsmasq 内置公开选项帮助字符串

- 原始文件：`usr/sbin/dnsmasq`。
- SHA-256：`f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`。
- 从 ELF 按 ASCII 字符串提取以下公开帮助文本；不运行 ARM 二进制。`%s` 是动态占位符，不把它当作可证明的默认值。

| 字段/选项 | 字符串文件偏移 | 原始帮助文本 |
| --- | --- | --- |
| `domainneeded` | `0x32bf2` | Do NOT forward queries with no domain part. |
| `filterwin2k` | `0x32c85` | Don't forward spurious DNS requests from Windows hosts. |
| `localise_queries` | `0x337aa` | Answer DNS queries based on the interface a query was sent to. |
| `expandhosts` | `0x32c4f` | Expand simple names in /etc/hosts with domain-suffix. |
| `authoritative` | `0x33092` | Assume we are the only DHCP server on the local network. |
| `readethers` | `0x3389c` | Read DHCP static host information from %s. |
| `noresolv` | `0x33332` | Do NOT read resolv.conf. |
| `localservice` | `0x34bca` | Accept queries only from directly-connected networks. |
| `strictorder` | `0x331d8` | Use nameservers strictly in the order given in %s. |
| `allservers` | `0x34036` | Always perform DNS queries to all servers. |
| `logqueries` | `0x332ec` | Log DNS queries. |
| `logdhcp` | `0x33f41` | Extra logging for DHCP. |
| `port` | `0x33270` | Specify port to listen for DNS requests on (defaults to 53). |
| `queryport` | `0x332fd` | Force the originating port for upstream DNS queries. |
| `cachesize` | `0x32b4d` | Specify the size of the cache in entries (defaults to %s). |
| `dnsforwardmax` | `0x33c29` | Maximum number of concurrent DNS queries. (defaults to %s) |
| `dhcpleasemax` | `0x33770` | Specify maximum number of DHCP leases (defaults to %s). |
| `ednspacket_max` | `0x332ad` | Maximum supported UDP packet size for EDNS.0 (defaults to %s). |
| `server` | `0x333a3` | Specify address(es) of upstream servers with optional domains. |
| `interface` | `0x32ea7` | Specify interface(s) to listen on. |
| `notinterface` | `0x32eca` | Specify interface(s) NOT to listen on. |
| `addnhosts` | `0x32e51` | Specify a hosts file to be read in addition to %s. |
| `rebind_protection` | `0x33f9a` | Stop DNS rebinding. Filter private IP ranges when resolving. |
| `rebind_localhost` | `0x33fd7` | Allow rebinding of 127.0.0.0/8, for RBL servers. |
| `rebind_domain` | `0x34008` | Inhibit DNS-rebind protection on this domain. |
| `local` | `0x33446` | Never forward queries to specified domains. |
| `domain` | `0x33485` | Specify the domain to be assigned in DHCP leases. |
| `dhcp_option` | `0x3320b` | Specify options to be sent to DHCP clients. |
| `nonwildcard` | `0x346ee` | Bind to interfaces in use - check for new interfaces |
| `dhcp_option_force` | `0x33237` | DHCP option sent even if the client does not request it. |
| `cname` | `0x342d2` | Specify alias name for LOCAL DNS name. |
| `cname_ttl_syntax` | `0x342b9` | <alias>,<target>[,<ttl>] |
| `relay` | `0x34272` | <local-addr>,<server>[,<iface>] |
| `host` | `0x32d4a` | Set address or hostname for a specified machine. |
| `broadcast` | `0x33027` | Force broadcast replies for hosts with tag set. |

## 覆盖与验证边界

- `dhcpFieldHelp` 共 93 个字段、10 种 section：dnsmasq 35、dhcp 25、host 9、domain 2、odhcpd 4、cname 3、boot 4、relay 3、srvhost 5、mxhost 3。
- 文本搜索范围：rootfs `etc/init.d`、`etc/hotplug.d`、`lib`、`usr/lib/lua`、`usr/share`、`usr/bin`、`usr/sbin`、`sbin` 中可解码文本，以及现有 `static/lua-analysis/decompiled` Lua。没有把当前 `/etc/config` 的值当作默认值。
- 后续准确性审查已在 `docs/field-help-queryport-review.md:113–178` 取得 ra_default 的 WAN6 门控、ra_slaac Autonomous 位、ra_mtu 下限、RA 间隔及有效期夹限分支，并同步到字段 metadata。仍为 binary-only 名称表或未取得完整运行分支的字段：dhcp.dns、domain；odhcpd.leasefile、leasetrigger、loglevel。master、ndp 有厂商写入与名称表，但内部中继/代理分支仍未完整取得。
- `ra_flags` 有厂商 set_list、模式分支与提交/restart 证据；`ra_maxinterval` 有仅针对 LAN 缺失时写 20 的 init 回退，未推断其他接口的缺省。
- 重要厂商差异：boguspriv 的读取被注释；启动 resolvfile 固定 `/tmp/resolv.conf.auto`；cname 生成器不读 ttl；host.broadcast 只加 needs-broadcast 标签而显式全局广播选项行已注释；LAN ignore 被启动前处理重写；安装的 odhcpd 为 ipv6only 变体。
- 版本比较仅读取父级已保存的 1.0.64 公共 dnsmasq 脚本副本：boguspriv/resolvfile/cname TTL 分支相同；LAN ignore 新增 product=ap 且 ft_mode=0 分支。未执行 SSH、服务操作或验证运行效果。
