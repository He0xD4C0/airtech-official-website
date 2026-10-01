use super::normalized_name;

pub(super) struct Definition {
    pub(super) key: &'static str,
    pub(super) required: bool,
    aliases: &'static [&'static str],
}

impl Definition {
    pub(super) fn match_score(&self, candidate: &str) -> Option<usize> {
        let candidate = normalized_name(candidate);
        self.aliases
            .iter()
            .map(|alias| normalized_name(alias))
            .filter_map(|alias| {
                let length = alias.chars().count();
                if candidate == alias {
                    Some(10_000 + length)
                } else if length >= 3 && candidate.contains(&alias) {
                    Some(length)
                } else {
                    None
                }
            })
            .max()
    }
}

pub(super) fn definitions() -> Vec<Definition> {
    vec![
        Definition {
            key: "model",
            required: true,
            aliases: &[
                "产品型号(Model）",
                "产品型号(Model)",
                "AIRTEK型号",
                "产品型号",
                "型号",
            ],
        },
        Definition {
            key: "category",
            required: false,
            aliases: &[
                "二级类_Product #2",
                "2级分类_category#2",
                "产品二级分类",
                "产品分类",
                "产品类别",
                "类别",
                "分类",
                "系列",
            ],
        },
        Definition {
            key: "primaryCategory",
            required: false,
            aliases: &[
                "一级分类_Product #1",
                "1级分类_category#1",
                "产品一级分类",
                "产品一级分类_Product #1",
                "产品分类（大类）",
            ],
        },
        Definition {
            key: "motorTechnology",
            required: false,
            aliases: &["电机技术", "电机类型", "Motor Technology"],
        },
        Definition {
            key: "voltage",
            required: false,
            aliases: &["额定电压", "电压(V)", "电压"],
        },
        Definition {
            key: "frequency",
            required: false,
            aliases: &["额定频率", "频率(Hz)", "频率"],
        },
        Definition {
            key: "speed",
            required: false,
            aliases: &["转速(rpm)", "转速", "额定转速"],
        },
        Definition {
            key: "current",
            required: false,
            aliases: &["电流(A)", "电流", "额定电流"],
        },
        Definition {
            key: "power",
            required: false,
            aliases: &["功率(W)", "输入功率", "额定功率", "功率"],
        },
        Definition {
            key: "airflow",
            required: false,
            aliases: &[
                "风量(Air Flow)",
                "风量(m³/h)",
                "风量(m3/h)",
                "最大风量",
                "风量",
            ],
        },
        Definition {
            key: "pressure",
            required: false,
            aliases: &["风压(AIr Pressure)", "风压(Pa)", "静压", "最大风压", "风压"],
        },
        Definition {
            key: "diameter",
            required: false,
            aliases: &["直径(mm)", "叶轮直径", "直径"],
        },
        Definition {
            key: "material",
            required: false,
            aliases: &["材质", "材料"],
        },
        Definition {
            key: "protection",
            required: false,
            aliases: &["防护等级", "IP等级", "防护"],
        },
        Definition {
            key: "insulation",
            required: false,
            aliases: &["绝缘等级", "绝缘"],
        },
        Definition {
            key: "ambientTemperature",
            required: false,
            aliases: &["Amb.Temp", "环境温度", "工作温度", "使用温度"],
        },
        Definition {
            key: "productDimensions",
            required: false,
            aliases: &["产品尺寸", "外形尺寸", "尺寸"],
        },
        Definition {
            key: "packageDimensions",
            required: false,
            aliases: &["包装尺寸", "包装规格", "包装数据Package data"],
        },
        Definition {
            key: "weight",
            required: false,
            aliases: &["重量weight(kg)", "产品净重", "净重", "重量", "Weight"],
        },
        Definition {
            key: "impellerLength",
            required: false,
            aliases: &["叶轮长度 Length", "叶轮长度"],
        },
        Definition {
            key: "dimensionA",
            required: false,
            aliases: &["尺寸:A"],
        },
        Definition {
            key: "dimensionB",
            required: false,
            aliases: &["尺寸:B"],
        },
        Definition {
            key: "dimensionC",
            required: false,
            aliases: &["尺寸:C"],
        },
        Definition {
            key: "status",
            required: false,
            aliases: &["当前状态", "产品状态", "状态"],
        },
        Definition {
            key: "noise",
            required: false,
            aliases: &["噪声", "噪音", "声压级"],
        },
        Definition {
            key: "testCondition",
            required: false,
            aliases: &["测试条件", "测试工况", "工况"],
        },
    ]
}
