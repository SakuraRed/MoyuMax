// MCMOD（mcmod.cn）收录热门模组的中文名与中文简介映射。
// 仅在简体/繁体中文界面展示;链接跳转 MCMOD 搜索页(条目 ID 无法从 slug 稳定推导,
// 不伪造深层链接)。未命中的项目不显示任何附加信息。

export interface McmodEntry {
  /** 中文显示名。 */
  zhName: string;
  /** 一句话中文简介。 */
  zhDescription: string;
  /** MCMOD 跳转链接(搜索页)。 */
  mcmodUrl: string;
}

function entry(zhName: string, zhDescription: string, slug: string): McmodEntry {
  return {
    zhName,
    zhDescription,
    mcmodUrl: `https://search.mcmod.cn/s?key=${encodeURIComponent(slug)}`,
  };
}

export const MCMOD_ENTRIES: Record<string, McmodEntry> = {
  "sodium": entry("钠 (Sodium)", "现代化渲染引擎，大幅提升帧率并改善画面稳定性。", "Sodium"),
  "lithium": entry("锂 (Lithium)", "不改动原版行为的游戏逻辑性能优化。", "Lithium"),
  "iris": entry("Iris 光影", "现代光影加载器，兼容 OptiFine 光影包。", "Iris Shaders"),
  "sodium-extra": entry("钠扩展 (Sodium Extra)", "为钠补充设置界面与细节画质选项。", "Sodium Extra"),
  "continuity": entry("连续性 (Continuity)", "为方块纹理提供连续连接效果。", "Continuity"),
  "fabric-api": entry("Fabric API", "Fabric 加载器官方 API，绝大多数 Fabric 模组的前置。", "Fabric API"),
  "modmenu": entry("模组菜单 (Mod Menu)", "在游戏内查看和管理已安装的模组。", "Mod Menu"),
  "cloth-config": entry("Cloth Config", "常见的模组配置界面库，许多模组的前置。", "Cloth Config"),
  "architectury-api": entry("Architectury", "跨加载器模组开发 API。", "Architectury"),
  "ferrite-core": entry("FerriteCore", "降低游戏内存占用。", "FerriteCore"),
  "krypton": entry("Krypton", "网络栈与性能优化。", "Krypton"),
  "starlight": entry("星光 (Starlight)", "重写光照引擎，显著提升光照计算性能。", "Starlight"),
  "immediatelyfast": entry("ImmediatelyFast", "界面与文本渲染优化。", "ImmediatelyFast"),
  "dynamic-fps": entry("动态帧率 (Dynamic FPS)", "窗口失焦时自动降低帧率，省电降噪。", "Dynamic FPS"),
  "entityculling": entry("实体剔除 (Entity Culling)", "不渲染被遮挡的实体，减轻渲染压力。", "Entity Culling"),
  "modernfix": entry("现代修复 (ModernFix)", "综合性能与稳定性改进。", "ModernFix"),
  "lazydfu": entry("LazyDFU", "延迟数据修复初始化，加快游戏启动。", "LazyDFU"),
  "create": entry("机械动力 (Create)", "传动、机械与自动化装置。", "Create"),
  "jei": entry("JEI 物品管理器", "查询物品、合成与用途（Just Enough Items）。", "JEI"),
  "roughly-enough-items": entry("REI 物品管理器", "物品与配方查询（Roughly Enough Items）。", "REI"),
  "appleskin": entry("苹果皮 (AppleSkin)", "显示饥饿度、饱和度与食物数值细节。", "AppleSkin"),
  "journeymap": entry("旅行地图 (JourneyMap)", "实时地图、小地图与路径点标记。", "JourneyMap"),
  "xaeros-minimap": entry("Xaero 小地图", "实体雷达式小地图。", "Xaero's Minimap"),
  "xaeros-world-map": entry("Xaero 世界地图", "可缩放的全屏世界地图。", "Xaero's World Map"),
  "waystones": entry("传送石 (Waystones)", "传送石碑、回城卷轴与绑定道具。", "Waystones"),
  "farmers-delight": entry("农夫乐事 (Farmer's Delight)", "扩展农业、烹饪与食物系统。", "Farmer's Delight"),
  "supplementaries": entry("锦致装饰 (Supplementaries)", "实用又美观的装饰与功能方块。", "Supplementaries"),
  "botania": entry("植物魔法 (Botania)", "以花朵与魔力为核心的魔法科技模组。", "Botania"),
  "twilight-forest": entry("暮色森林 (Twilight Forest)", "全新维度、地牢与 Boss 冒险内容。", "Twilight Forest"),
  "biomes-o-plenty": entry("超多生物群系 (Biomes O' Plenty)", "新增数十种生物群系。", "Biomes O' Plenty"),
  "terrablender": entry("TerraBlender", "世界生成兼容库，超多生物群系的前置。", "TerraBlender"),
  "curios": entry("饰品栏 (Curios)", "饰品与配饰槽位 API。", "Curios"),
  "patchouli": entry("帕秋莉手册 (Patchouli)", "游戏内文档手册库。", "Patchouli"),
  "geckolib": entry("GeckoLib", "骨骼动画与模型库。", "GeckoLib"),
  "malilib": entry("MaLiLib", "投影等模组共用的基础库。", "MaLiLib"),
  "litematica": entry("投影 (Litematica)", "结构投影与建造辅助。", "Litematica"),
  "worldedit": entry("创世神 (WorldEdit)", "强大的地图编辑工具。", "WorldEdit"),
  "carpet": entry("地毯 (Carpet)", "技术向游戏规则与特性控制。", "Carpet"),
  "carpet-extra": entry("地毯扩展 (Carpet Extra)", "Carpet 的扩展规则集。", "Carpet Extra"),
  "carpet-tis-addition": entry("TIS 地毯附加 (Carpet TIS Addition)", "TIS 服务器的 Carpet 附加规则。", "Carpet TIS Addition"),
  "spark": entry("spark", "游戏性能分析工具。", "spark"),
  "corpse": entry("遗体 (Corpse)", "死亡后留下保存物品的遗体。", "Corpse"),
  "corail-tombstone": entry("Corail 墓碑", "死亡生成墓碑，可寻回物品。", "Corail Tombstone"),
  "inventory-sorter": entry("一键整理 (Inventory Sorter)", "背包与容器一键整理。", "Inventory Sorter"),
  "shulkerboxtooltip": entry("潜影盒预览 (ShulkerBoxTooltip)", "悬停即可查看潜影盒内容。", "ShulkerBoxTooltip"),
  "jade": entry("玉 (Jade)", "方块与实体信息高亮显示（WAILA 系）。", "Jade"),
  "applied-energistics-2": entry("应用能源 2 (AE2)", "数字化存储与自动化网络。", "Applied Energistics 2"),
  "mekanism": entry("通用机械 (Mekanism)", "工业、能源与化工体系。", "Mekanism"),
  "tinkers-construct": entry("匠魂 (Tinkers' Construct)", "自定义工具、武器与冶炼系统。", "Tinkers' Construct"),
  "quark": entry("夸克 (Quark)", "大量小而美的特性集合。", "Quark"),
};

/** 按 Modrinth slug 查 MCMOD 中文条目;未收录返回 null。 */
export function mcmodEntryFor(slug: string): McmodEntry | null {
  return MCMOD_ENTRIES[slug] ?? null;
}
