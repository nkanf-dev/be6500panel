-- One-shot factory controller adapter. The Rust API validates a fixed action
-- and typed fields. Credentials stay on stdin and are never written to argv.
local modules = {
  xqnetwork = "luci.controller.api.xqnetwork",
  xqsystem = "luci.controller.api.xqsystem",
  misystem = "luci.controller.api.misystem",
  misns = "luci.controller.api.misns",
  anti_attack = "luci.controller.anti_attack.index",
  sysutil = "xiaoqiang.util.XQSysUtil"
}
local controller,handler=arg[1],arg[2]
-- Keep the reply on a separate descriptor. Native C UCI helpers and spawned
-- commands also write directly to fd 1, beyond Lua print/io.write overrides.
local nixio=require("nixio")
local reply=assert(nixio.dup(nixio.stdout))
local sink=assert(nixio.open("/dev/null","w"))
assert(nixio.dup(sink,nixio.stdout))
sink:close()
local function raw_write(value) assert(reply:writeall(value)) end
local json=require("cjson")
local function fail(code)
  raw_write(json.encode({code=code}))
  raw_write("\n")
end
if not modules[controller] or not handler or not handler:match("^[A-Za-z][A-Za-z0-9_]*$") then
  fail(400); os.exit(1)
end
-- Exact controller-function allowlist is inserted by the release builder.
local allowed = {
  ["anti_attack:get_status_api"] = true,
  ["anti_attack:set_dos_api"] = true,
  ["anti_attack:set_rpfilter_api"] = true,
  ["anti_attack:set_scan_api"] = true,
  ["misns:wifiShareInfo"] = true,
  ["misystem:DoAllLED"] = true,
  ["misystem:DoEthLED"] = true,
  ["misystem:getMACQoSInfo"] = true,
  ["misystem:getMeshBhMode"] = true,
  ["misystem:getPSMap"] = true,
  ["misystem:getPSService"] = true,
  ["misystem:getPctlApp"] = true,
  ["misystem:getPctlBanHost"] = true,
  ["misystem:getPctlDenyTime"] = true,
  ["misystem:getPctlDev"] = true,
  ["misystem:getPctlTempBan"] = true,
  ["misystem:getPctlUserList"] = true,
  ["misystem:getQos"] = true,
  ["misystem:getQosInfo"] = true,
  ["misystem:getRouterName"] = true,
  ["misystem:getSysTime"] = true,
  ["misystem:getTopoGraph"] = true,
  ["misystem:getVlanIPTV"] = true,
  ["misystem:getVlanInternet"] = true,
  ["misystem:getWebAccessInfo"] = true,
  ["misystem:hwnatStatus"] = true,
  ["misystem:ledCtl"] = true,
  ["misystem:qosGuest"] = true,
  ["misystem:qosLimit"] = true,
  ["misystem:qosLimits"] = true,
  ["misystem:qosMode"] = true,
  ["misystem:qosOffLimit"] = true,
  ["misystem:qosSwitch"] = true,
  ["misystem:setBand"] = true,
  ["misystem:setMACQoSInfo"] = true,
  ["misystem:setMeshBhMode"] = true,
  ["misystem:setPSService"] = true,
  ["misystem:setPctl"] = true,
  ["misystem:setRouterName"] = true,
  ["misystem:setSysTime"] = true,
  ["misystem:setVlanService"] = true,
  ["misystem:webAccess"] = true,
  ["sysutil:getNTPServerList"] = true,
  ["sysutil:setNTPServer"] = true,
  ["sysutil:timeMode"] = true,
  ["xqnetwork:addMeshNode"] = true,
  ["xqnetwork:addServer"] = true,
  ["xqnetwork:ddnsEdit"] = true,
  ["xqnetwork:ddnsReload"] = true,
  ["xqnetwork:ddnsStatus"] = true,
  ["xqnetwork:deleteServer"] = true,
  ["xqnetwork:disableLanAP"] = true,
  ["xqnetwork:disableap"] = true,
  ["xqnetwork:editDevice"] = true,
  ["xqnetwork:getAllWifiInfo"] = true,
  ["xqnetwork:getBridgeLanStatus"] = true,
  ["xqnetwork:getGwSecurity"] = true,
  ["xqnetwork:getHostapMLO"] = true,
  ["xqnetwork:getIPMACCheckStatus"] = true,
  ["xqnetwork:getIpv6Firewall"] = true,
  ["xqnetwork:getLan6V2"] = true,
  ["xqnetwork:getLanDhcp"] = true,
  ["xqnetwork:getLanInfo"] = true,
  ["xqnetwork:getMacBindInfo"] = true,
  ["xqnetwork:getMeshNodeStatus"] = true,
  ["xqnetwork:getMeshSwitch"] = true,
  ["xqnetwork:getMiotrelaySwitch"] = true,
  ["xqnetwork:getMiscanSwitch"] = true,
  ["xqnetwork:getMode"] = true,
  ["xqnetwork:getMultiwanBasicInfo"] = true,
  ["xqnetwork:getMultiwanDevList"] = true,
  ["xqnetwork:getMultiwanDevPolicies"] = true,
  ["xqnetwork:getNetMode"] = true,
  ["xqnetwork:getServer"] = true,
  ["xqnetwork:getTwt"] = true,
  ["xqnetwork:getWan6InfoV2"] = true,
  ["xqnetwork:getWan6SwitchV2"] = true,
  ["xqnetwork:getWan6V2"] = true,
  ["xqnetwork:getWanInfo"] = true,
  ["xqnetwork:getWanLinkStatus"] = true,
  ["xqnetwork:getWanSpeed"] = true,
  ["xqnetwork:getWanStatus"] = true,
  ["xqnetwork:getWifiChTx"] = true,
  ["xqnetwork:getWifiConDev"] = true,
  ["xqnetwork:getWifiInfo"] = true,
  ["xqnetwork:getWifiMacfilterInfo"] = true,
  ["xqnetwork:getWifiStatus"] = true,
  ["xqnetwork:getWifiWeakInfo"] = true,
  ["xqnetwork:macBind"] = true,
  ["xqnetwork:macUnbind"] = true,
  ["xqnetwork:manuallyAdd"] = true,
  ["xqnetwork:miotrelaySwitch"] = true,
  ["xqnetwork:miscanSwitch"] = true,
  ["xqnetwork:pppoeStart"] = true,
  ["xqnetwork:pppoeStatus"] = true,
  ["xqnetwork:pppoeStop"] = true,
  ["xqnetwork:scanMeshNode"] = true,
  ["xqnetwork:serverSwitch"] = true,
  ["xqnetwork:setAllWifi"] = true,
  ["xqnetwork:setGwSecurity"] = true,
  ["xqnetwork:setHostapMLO"] = true,
  ["xqnetwork:setIPMACCheckEnable"] = true,
  ["xqnetwork:setIpv6Firewall"] = true,
  ["xqnetwork:setLan6V2"] = true,
  ["xqnetwork:setLanAP"] = true,
  ["xqnetwork:setLanDhcp"] = true,
  ["xqnetwork:setLanIp"] = true,
  ["xqnetwork:setMeshSwitch"] = true,
  ["xqnetwork:setMultiwanDevPolicy"] = true,
  ["xqnetwork:setMultiwanEnable"] = true,
  ["xqnetwork:setMultiwanPolicy"] = true,
  ["xqnetwork:setMultiwanWeight"] = true,
  ["xqnetwork:setTwt"] = true,
  ["xqnetwork:setWan"] = true,
  ["xqnetwork:setWan6SwitchV2"] = true,
  ["xqnetwork:setWan6V2"] = true,
  ["xqnetwork:setWanMac"] = true,
  ["xqnetwork:setWanSpeed"] = true,
  ["xqnetwork:setWifi"] = true,
  ["xqnetwork:setWifiApMode"] = true,
  ["xqnetwork:setWifiAx"] = true,
  ["xqnetwork:setWifiMacfilter"] = true,
  ["xqnetwork:setWifiTxbf"] = true,
  ["xqnetwork:setWifiTxpwr"] = true,
  ["xqnetwork:setWifiWeakInfo"] = true,
  ["xqnetwork:setWifiWithoutRestart"] = true,
  ["xqnetwork:wanDown"] = true,
  ["xqnetwork:wanUp"] = true,
  ["xqsystem:addRangeRedirect"] = true,
  ["xqsystem:addRedirect"] = true,
  ["xqsystem:checkRomUpdate"] = true,
  ["xqsystem:closeDMZ"] = true,
  ["xqsystem:deleteRedirect"] = true,
  ["xqsystem:flashRom"] = true,
  ["xqsystem:getDMZInfo"] = true,
  ["xqsystem:getForceHttps"] = true,
  ["xqsystem:getInitInfo"] = true,
  ["xqsystem:getOTAInfo"] = true,
  ["xqsystem:get_dos_firewall"] = true,
  ["xqsystem:get_firewall_enable"] = true,
  ["xqsystem:get_spi_firewall"] = true,
  ["xqsystem:get_wanping_firewall"] = true,
  ["xqsystem:portForward"] = true,
  ["xqsystem:reboot"] = true,
  ["xqsystem:redirectApply"] = true,
  ["xqsystem:reloadDMZ"] = true,
  ["xqsystem:reset"] = true,
  ["xqsystem:setDMZ"] = true,
  ["xqsystem:setForceHttps"] = true,
  ["xqsystem:setOTAInfo"] = true,
  ["xqsystem:set_dos_firewall"] = true,
  ["xqsystem:set_firewall_enable"] = true,
  ["xqsystem:set_spi_firewall"] = true,
  ["xqsystem:set_wanping_firewall"] = true,
  ["xqsystem:upgradeStatus"] = true,
  ["xqsystem:upgradeRom"] = true,
  ["xqsystem:upnpList"] = true,
  ["xqsystem:upnpSwitch"] = true
}
if not allowed[controller .. ":" .. handler] then fail(400); os.exit(1) end
local raw = io.read(65537)
if not raw or #raw > 65536 then fail(400); os.exit(1) end
local ok, input = pcall(json.decode, raw)
if not ok or type(input) ~= "table" then fail(400); os.exit(1) end
-- LuCI normally supplies translation helpers through its dispatcher.
_G._=function(text)return text end
_G.translate=_G._
local captured
local http=require("luci.http")
http.formvalue = function(key)
  local value = input[key]
  if value == nil or value == json.null then return nil end
  if type(value) == "table" then return json.encode(value) end
  if type(value) == "boolean" then return value and "1" or "0" end
  return tostring(value)
end
http.formvaluetable = function(prefix)
  local value = input[prefix]
  return type(value) == "table" and value or {}
end
http.write_json = function(value) captured = value end
http.prepare_content = function() end
http.header = function() end
http.status = function() end
http.write=function()end
http.close=function()end
-- Native helpers may print debug text; it is not part of the public reply.
print=function()end
io.write=function()end
local loaded, module = pcall(require, modules[controller])
if not loaded or type(module) ~= "table" then fail(503); os.exit(1) end
local success, result
if controller == "sysutil" then
  if handler == "getNTPServerList" then
    success, result = pcall(function() return {code=0,servers=module.getNTPServerList()} end)
  elseif handler == "setNTPServer" then
    success, result = pcall(function()
      local applied = module.setNTPServer(input.server1, input.server2)
      return {code=applied and 0 or 1,servers=module.getNTPServerList()}
    end)
  elseif handler == "timeMode" then
    success, result = pcall(function() return {code=0,info=module.timeMode(input.sync and tostring(input.sync) or nil,input.mode and tostring(input.mode) or nil)} end)
  end
elseif controller=="xqnetwork" and handler=="ddnsStatus" then
  success,result=pcall(function()
    local info=require("xiaoqiang.module.XQDDNS").ddnsInfo()
    return {code=0,list=info}
  end)
else
  if type(module[handler]) ~= "function" then fail(404); os.exit(1) end
  success, result = pcall(module[handler])
end
if not success then fail(503); os.exit(1) end
result = captured or result
if type(result) ~= "table" then fail(502); os.exit(1) end
local encoded, output = pcall(json.encode, result)
if not encoded or #output > 262144 then fail(502); os.exit(1) end
raw_write(output);raw_write("\n")
