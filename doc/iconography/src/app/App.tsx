import { OSLogo } from "./components/OSLogo";
import { OSLogoStatic } from "./components/OSLogoStatic";
import { OSLogoBW } from "./components/OSLogoBW";
import { OSLogoIcon } from "./components/OSLogoIcon";
import { AnimatedBubbles } from "./components/AnimatedBubbles";

export default function App() {
  return (
    <div className="size-full bg-black p-12 overflow-auto">
      <div className="max-w-7xl mx-auto space-y-16">
        {/* Full animated version */}
        <div className="flex flex-col items-center gap-4">
          <OSLogo />
        </div>
        
        {/* Divider */}
        <div className="border-t border-white/20" />
        
        {/* Simplified versions */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-12">
          {/* Static version */}
          <div className="flex flex-col items-center gap-4">
            <OSLogoStatic />
            <p className="text-white/60 text-sm">Static (No Animation)</p>
          </div>
          
          {/* Black and white version */}
          <div className="flex flex-col items-center gap-4">
            <OSLogoBW />
            <p className="text-white/60 text-sm">Black & White</p>
          </div>
          
          {/* Icon version */}
          <div className="flex flex-col items-center gap-4">
            <div className="h-[200px] flex items-center justify-center">
              <OSLogoIcon />
            </div>
            <p className="text-white/60 text-sm">Taskbar Icon (32x32)</p>
          </div>
        </div>

        {/* Divider */}
        <div className="border-t border-white/20" />

        {/* Animated Bubbles Section */}
        <div className="flex flex-col items-center gap-8">
          <h2 className="text-2xl text-white font-light">Animated Bubbles</h2>
          <AnimatedBubbles />
        </div>
      </div>
    </div>
  );
}