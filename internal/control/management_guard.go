package control

import "reflect"

// LAN listener migration is a separate coordinated platform capability. A raw
// native editor cannot safely change the panel's active address or bridge yet.
func validateLANManagement(before, after string) []Issue {
	old, oldErr := parse(before)
	next, nextErr := parse(after)
	if oldErr != nil || nextErr != nil {
		return []Issue{issue("management_migration_unavailable", "当前面板尚不支持迁移 LAN 管理地址；请保留当前管理接口。")}
	}
	project := func(doc parsed) map[string]map[string][]string {
		result := map[string]map[string][]string{}
		for _, section := range doc {
			if section.kind == "interface" && section.name == "lan" {
				fields := map[string][]string{}
				for _, name := range []string{"device", "ifname", "proto", "ipaddr", "netmask", "ip6addr", "ip6assign", "ip6hint", "disabled", "auto", "type"} {
					if values, ok := section.values[name]; ok {
						fields[name] = append([]string{}, values...)
					}
				}
				result["lan"] = fields
			}
		}
		return result
	}
	if !reflect.DeepEqual(project(old), project(next)) {
		return []Issue{issue("management_migration_unavailable", "当前面板尚不支持迁移 LAN 管理地址或网桥。其他设置仍可编辑；管理地址迁移请使用路由器原厂页面。")}
	}
	// Changes to the actual device section can remove the managed bridge even
	// when the logical lan interface still names it.
	devices := func(doc parsed) map[string]map[string][]string {
		result := map[string]map[string][]string{}
		names := map[string]bool{}
		for _, section := range doc {
			if section.kind == "interface" && section.name == "lan" {
				for _, key := range []string{"device", "ifname"} {
					for _, name := range section.values[key] {
						names[name] = true
					}
				}
			}
		}
		for _, section := range doc {
			if section.kind == "device" && names[one(section, "name")] {
				result[one(section, "name")] = section.values
			}
		}
		return result
	}
	if !reflect.DeepEqual(devices(old), devices(next)) {
		return []Issue{issue("management_migration_unavailable", "当前面板尚不支持迁移 LAN 管理网桥；请保留其设备设置。")}
	}
	return nil
}
