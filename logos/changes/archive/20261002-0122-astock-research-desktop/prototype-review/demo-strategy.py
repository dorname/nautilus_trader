# 日序研究接口草案 · 仅作编辑器示例
# history: {股票代码: 按时间升序排列的收盘价}
# parameters: 来自参数定义中的默认值
# 正式接口与时点数据校验尚未实现

def generate_targets(history, parameters):
    lookback = parameters["lookback"]
    top_k = parameters["top_k"]
    scores = []

    for symbol, prices in history.items():
        if len(prices) <= lookback:
            continue
        score = prices[-1] / prices[-lookback - 1] - 1
        if score > 0:
            scores.append((symbol, score))

    scores.sort(key=lambda item: (-item[1], item[0]))
    selected = scores[:top_k]
    return {symbol: 1.0 / top_k for symbol, _ in selected}

# 编辑保留检查
# 新改动
# 尚未保存的新草稿
# 再次修改
