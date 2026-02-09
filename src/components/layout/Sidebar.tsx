import { NavLink } from "react-router-dom";
import { cn } from "@/lib/utils";
import {
  LayoutDashboard,
  Upload,
  Users,
  FileText,
  Clock,
  Settings,
  Sun,
  Moon,
  Monitor,
} from "lucide-react";
import { useThemeStore } from "@/stores/themeStore";

const navItems = [
  { to: "/", icon: LayoutDashboard, label: "仪表盘" },
  { to: "/publish", icon: Upload, label: "发布中心" },
  { to: "/accounts", icon: Users, label: "账号管理" },
  { to: "/templates", icon: FileText, label: "模板" },
  { to: "/history", icon: Clock, label: "历史记录" },
];

export function Sidebar() {
  const { theme, setTheme } = useThemeStore();

  const cycleTheme = () => {
    if (theme === "light") setTheme("dark");
    else if (theme === "dark") setTheme("system");
    else setTheme("light");
  };

  const ThemeIcon = theme === "light" ? Sun : theme === "dark" ? Moon : Monitor;

  return (
    <aside className="flex flex-col items-center w-[60px] bg-sidebar border-r border-border py-4 gap-1">
      {/* Logo */}
      <div className="w-9 h-9 rounded-xl bg-primary flex items-center justify-center mb-4">
        <span className="text-primary-foreground font-bold text-sm">MP</span>
      </div>

      {/* Navigation */}
      <nav className="flex flex-col items-center gap-1 flex-1">
        {navItems.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              cn(
                "w-10 h-10 rounded-lg flex items-center justify-center transition-all duration-200 group relative",
                isActive
                  ? "bg-primary text-primary-foreground shadow-md"
                  : "text-sidebar-foreground hover:bg-accent hover:text-accent-foreground"
              )
            }
          >
            <item.icon className="w-5 h-5" />
            {/* Tooltip */}
            <span className="absolute left-full ml-2 px-2 py-1 bg-popover text-popover-foreground text-xs rounded-md shadow-lg opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 border border-border">
              {item.label}
            </span>
          </NavLink>
        ))}
      </nav>

      {/* Theme toggle at bottom */}
      <button
        onClick={cycleTheme}
        className="w-10 h-10 rounded-lg flex items-center justify-center text-sidebar-foreground hover:bg-accent hover:text-accent-foreground transition-colors group relative"
      >
        <ThemeIcon className="w-5 h-5" />
        <span className="absolute left-full ml-2 px-2 py-1 bg-popover text-popover-foreground text-xs rounded-md shadow-lg opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 border border-border">
          {theme === "light" ? "浅色" : theme === "dark" ? "深色" : "跟随系统"}
        </span>
      </button>

      {/* Settings */}
      <NavLink
        to="/settings"
        className={({ isActive }) =>
          cn(
            "w-10 h-10 rounded-lg flex items-center justify-center transition-colors group relative",
            isActive
              ? "bg-primary text-primary-foreground"
              : "text-sidebar-foreground hover:bg-accent hover:text-accent-foreground"
          )
        }
      >
        <Settings className="w-5 h-5" />
        <span className="absolute left-full ml-2 px-2 py-1 bg-popover text-popover-foreground text-xs rounded-md shadow-lg opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 border border-border">
          设置
        </span>
      </NavLink>
    </aside>
  );
}
