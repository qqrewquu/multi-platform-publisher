import { Plus, FileText } from "lucide-react";

export function Templates() {
  return (
    <div className="p-8 max-w-5xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-foreground">模板管理</h1>
          <p className="text-sm text-muted-foreground mt-1">
            创建和管理你的内容模板，快速填充发布信息
          </p>
        </div>
        <button className="inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
          <Plus className="w-4 h-4" />
          新建模板
        </button>
      </div>

      {/* Empty State */}
      <div className="bg-card border border-border rounded-xl p-12 text-center">
        <FileText className="w-12 h-12 text-muted-foreground mx-auto mb-4" />
        <h3 className="text-lg font-semibold text-foreground mb-2">
          还没有模板
        </h3>
        <p className="text-sm text-muted-foreground mb-4 max-w-sm mx-auto">
          创建内容模板来保存常用的标题、描述和标签，发布时一键套用
        </p>
        <button className="inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
          <Plus className="w-4 h-4" />
          创建第一个模板
        </button>
      </div>
    </div>
  );
}
