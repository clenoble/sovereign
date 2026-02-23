export function AnimatedBubbles() {
  return (
    <div className="grid grid-cols-4 gap-8 p-8">
      {/* Bubble 1: Wave Animation - Blue */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <clipPath id="bubbleClip1">
              <circle cx="60" cy="60" r="50" />
            </clipPath>
            <linearGradient id="waveGradient" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#60A5FA" />
              <stop offset="100%" stopColor="#3B82F6" />
            </linearGradient>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="#3B82F6" opacity="0.3" />
          
          <g clipPath="url(#bubbleClip1)">
            {/* Sinusoidal wave lines */}
            <path d="M 0 60 Q 15 50, 30 60 T 60 60 T 90 60 T 120 60" fill="none" stroke="#60A5FA" strokeWidth="2" opacity="0.8">
              <animate attributeName="d" 
                values="M 0 60 Q 15 50, 30 60 T 60 60 T 90 60 T 120 60;
                        M 0 60 Q 15 70, 30 60 T 60 60 T 90 60 T 120 60;
                        M 0 60 Q 15 50, 30 60 T 60 60 T 90 60 T 120 60"
                dur="2s" repeatCount="indefinite" />
            </path>
            
            <path d="M 0 50 Q 15 40, 30 50 T 60 50 T 90 50 T 120 50" fill="none" stroke="#3B82F6" strokeWidth="2" opacity="0.6">
              <animate attributeName="d" 
                values="M 0 50 Q 15 40, 30 50 T 60 50 T 90 50 T 120 50;
                        M 0 50 Q 15 60, 30 50 T 60 50 T 90 50 T 120 50;
                        M 0 50 Q 15 40, 30 50 T 60 50 T 90 50 T 120 50"
                dur="2.3s" repeatCount="indefinite" />
            </path>
            
            <path d="M 0 70 Q 15 60, 30 70 T 60 70 T 90 70 T 120 70" fill="none" stroke="#60A5FA" strokeWidth="2" opacity="0.6">
              <animate attributeName="d" 
                values="M 0 70 Q 15 60, 30 70 T 60 70 T 90 70 T 120 70;
                        M 0 70 Q 15 80, 30 70 T 60 70 T 90 70 T 120 70;
                        M 0 70 Q 15 60, 30 70 T 60 70 T 90 70 T 120 70"
                dur="2.7s" repeatCount="indefinite" />
            </path>
            
            {/* Center dot */}
            <circle cx="60" cy="60" r="3" fill="#60A5FA">
              <animate attributeName="opacity" values="1;0.5;1" dur="1.5s" repeatCount="indefinite" />
            </circle>
          </g>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="#60A5FA" strokeWidth="2" opacity="0.5" />
        </svg>
        <span className="text-sm text-gray-300">Wave</span>
      </div>

      {/* Bubble 2: Spinning Curves - Purple */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <linearGradient id="purpleGradient" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#8B5CF6" />
              <stop offset="100%" stopColor="#7C3AED" />
            </linearGradient>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="url(#purpleGradient)" opacity="0.3" />
          
          <g transform-origin="60 60">
            {/* Petal 1 - 0 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.9">
              <animateTransform attributeName="transform" type="rotate" 
                values="0 60 60;360 60 60"
                dur="14s" repeatCount="indefinite" />
            </ellipse>
            
            {/* Petal 2 - 22.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.5" transform="rotate(22.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="22.5 60 60;382.5 60 60"
                dur="19s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 3 - 45 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.7" transform="rotate(45 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="45 60 60;405 60 60"
                dur="12.4s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 4 - 67.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.8" transform="rotate(67.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="67.5 60 60;427.5 60 60"
                dur="17.6s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 5 - 90 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.6" transform="rotate(90 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="90 60 60;450 60 60"
                dur="20s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 6 - 112.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.75" transform="rotate(112.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="112.5 60 60;472.5 60 60"
                dur="14.6s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 7 - 135 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.85" transform="rotate(135 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="135 60 60;495 60 60"
                dur="11.6s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 8 - 157.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.55" transform="rotate(157.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="157.5 60 60;517.5 60 60"
                dur="18s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 9 - 180 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.65" transform="rotate(180 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="180 60 60;540 60 60"
                dur="13.4s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 10 - 202.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.9" transform="rotate(202.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="202.5 60 60;562.5 60 60"
                dur="16.4s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 11 - 225 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.5" transform="rotate(225 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="225 60 60;585 60 60"
                dur="15.4s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 12 - 247.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.7" transform="rotate(247.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="247.5 60 60;607.5 60 60"
                dur="18.6s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 13 - 270 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.8" transform="rotate(270 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="270 60 60;630 60 60"
                dur="13s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 14 - 292.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.6" transform="rotate(292.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="292.5 60 60;652.5 60 60"
                dur="17s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 15 - 315 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#8B5CF6" opacity="0.75" transform="rotate(315 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="315 60 60;675 60 60"
                dur="21s" repeatCount="indefinite" additive="replace" />
            </ellipse>
            
            {/* Petal 16 - 337.5 degrees */}
            <ellipse cx="60" cy="30" rx="8" ry="20" fill="#7C3AED" opacity="0.85" transform="rotate(337.5 60 60)">
              <animateTransform attributeName="transform" type="rotate" 
                values="337.5 60 60;697.5 60 60"
                dur="15.6s" repeatCount="indefinite" additive="replace" />
            </ellipse>
          </g>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="#8B5CF6" strokeWidth="2" opacity="0.5" />
        </svg>
        <span className="text-sm text-gray-300">Spin</span>
      </div>

      {/* Bubble 3: Gradient Animation - Pink */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <radialGradient id="pinkGradient">
              <stop offset="0%" stopColor="#EC4899">
                <animate attributeName="stop-color" 
                  values="#EC4899;#F472B6;#FB7185;#F472B6;#EC4899;#BE185D;#9F1239;#BE185D;#EC4899" 
                  dur="12s" repeatCount="indefinite" />
              </stop>
              <stop offset="50%" stopColor="#DB2777">
                <animate attributeName="stop-color" 
                  values="#DB2777;#EC4899;#F472B6;#EC4899;#DB2777;#BE185D;#DB2777" 
                  dur="9s" repeatCount="indefinite" />
                <animate attributeName="offset"
                  values="50%;60%;40%;70%;50%;30%;50%"
                  dur="11s" repeatCount="indefinite" />
              </stop>
              <stop offset="100%" stopColor="#BE185D">
                <animate attributeName="stop-color" 
                  values="#BE185D;#9F1239;#831843;#BE185D;#EC4899;#F472B6;#BE185D" 
                  dur="10s" repeatCount="indefinite" />
              </stop>
            </radialGradient>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="url(#pinkGradient)">
            <animate attributeName="opacity" 
              values="0.7;0.9;0.75;1;0.65;0.85;0.7" 
              dur="12s" repeatCount="indefinite" />
          </circle>
          
          {/* Fast pulse ring */}
          <circle cx="60" cy="60" r="35" fill="none" stroke="#F472B6" strokeWidth="2" opacity="0.6">
            <animate attributeName="r" values="35;42;35" dur="1.8s" repeatCount="indefinite" />
            <animate attributeName="opacity" values="0.6;0.1;0.6" dur="1.8s" repeatCount="indefinite" />
          </circle>
          
          {/* Medium pulse ring */}
          <circle cx="60" cy="60" r="28" fill="none" stroke="#EC4899" strokeWidth="2" opacity="0.5">
            <animate attributeName="r" values="28;36;28" dur="3.2s" repeatCount="indefinite" begin="0.5s" />
            <animate attributeName="opacity" values="0.5;0.15;0.5" dur="3.2s" repeatCount="indefinite" begin="0.5s" />
            <animate attributeName="stroke" 
              values="#EC4899;#F472B6;#DB2777;#EC4899" 
              dur="8s" repeatCount="indefinite" />
          </circle>
          
          {/* Slow dramatic pulse ring */}
          <circle cx="60" cy="60" r="40" fill="none" stroke="#BE185D" strokeWidth="3" opacity="0.4">
            <animate attributeName="r" values="40;48;40" dur="5.5s" repeatCount="indefinite" begin="1s" />
            <animate attributeName="opacity" values="0.4;0.05;0.4" dur="5.5s" repeatCount="indefinite" begin="1s" />
            <animate attributeName="stroke" 
              values="#BE185D;#9F1239;#FB7185;#BE185D" 
              dur="6s" repeatCount="indefinite" />
          </circle>
          
          {/* Subtle inner pulse */}
          <circle cx="60" cy="60" r="20" fill="none" stroke="#F472B6" strokeWidth="1.5" opacity="0.7">
            <animate attributeName="r" values="20;22;20" dur="2.5s" repeatCount="indefinite" begin="0.3s" />
            <animate attributeName="opacity" values="0.7;0.3;0.7" dur="2.5s" repeatCount="indefinite" begin="0.3s" />
          </circle>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="#EC4899" strokeWidth="2" opacity="0.5" />
        </svg>
        <span className="text-sm text-gray-300">Pulse</span>
      </div>

      {/* Bubble 4: Blinking Dots - Gold */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <radialGradient id="goldGradient">
              <stop offset="0%" stopColor="#FCD34D" />
              <stop offset="100%" stopColor="#F59E0B" />
            </radialGradient>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="url(#goldGradient)" opacity="0.3" />
          
          <circle cx="60" cy="25" r="4" fill="#FCD34D">
            <animate attributeName="opacity" values="1;0.2;1" dur="3s" repeatCount="indefinite" begin="0s" />
          </circle>
          <circle cx="85" cy="37" r="4" fill="#F59E0B">
            <animate attributeName="opacity" values="1;0.2;1" dur="3.5s" repeatCount="indefinite" begin="0.7s" />
          </circle>
          <circle cx="95" cy="60" r="4" fill="#FCD34D">
            <animate attributeName="opacity" values="1;0.2;1" dur="2.8s" repeatCount="indefinite" begin="1.2s" />
          </circle>
          <circle cx="85" cy="83" r="4" fill="#F59E0B">
            <animate attributeName="opacity" values="1;0.2;1" dur="3.2s" repeatCount="indefinite" begin="0.4s" />
          </circle>
          <circle cx="60" cy="95" r="4" fill="#FCD34D">
            <animate attributeName="opacity" values="1;0.2;1" dur="2.9s" repeatCount="indefinite" begin="1.8s" />
          </circle>
          <circle cx="35" cy="83" r="4" fill="#F59E0B">
            <animate attributeName="opacity" values="1;0.2;1" dur="3.3s" repeatCount="indefinite" begin="2.1s" />
          </circle>
          <circle cx="25" cy="60" r="4" fill="#FCD34D">
            <animate attributeName="opacity" values="1;0.2;1" dur="3.1s" repeatCount="indefinite" begin="0.9s" />
          </circle>
          <circle cx="35" cy="37" r="4" fill="#F59E0B">
            <animate attributeName="opacity" values="1;0.2;1" dur="2.7s" repeatCount="indefinite" begin="1.5s" />
          </circle>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="#F59E0B" strokeWidth="2" opacity="0.5" />
        </svg>
        <span className="text-sm text-gray-300">Blink</span>
      </div>

      {/* Bubble 5: Rotating Rings - Green */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <linearGradient id="greenGradient" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#10B981" />
              <stop offset="100%" stopColor="#34D399" />
            </linearGradient>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="url(#greenGradient)" opacity="0.3" />
          
          <g transform-origin="60 60">
            <ellipse cx="60" cy="60" rx="40" ry="15" fill="none" stroke="#10B981" strokeWidth="2" opacity="0.7">
              <animateTransform attributeName="transform" type="rotate" 
                values="0 60 60;120 60 60;360 60 60;240 60 60;0 60 60"
                dur="10s" repeatCount="indefinite" />
            </ellipse>
          </g>
          
          <g transform-origin="60 60">
            <ellipse cx="60" cy="60" rx="40" ry="15" fill="none" stroke="#34D399" strokeWidth="2" opacity="0.7">
              <animateTransform attributeName="transform" type="rotate" 
                values="60 60 60;240 60 60;420 60 60;180 60 60;60 60 60"
                dur="11s" repeatCount="indefinite" />
            </ellipse>
          </g>
          
          <g transform-origin="60 60">
            <ellipse cx="60" cy="60" rx="40" ry="15" fill="none" stroke="#10B981" strokeWidth="2" opacity="0.7">
              <animateTransform attributeName="transform" type="rotate" 
                values="120 60 60;300 60 60;480 60 60;360 60 60;120 60 60"
                dur="9s" repeatCount="indefinite" />
            </ellipse>
          </g>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="#10B981" strokeWidth="2" opacity="0.5" />
        </svg>
        <span className="text-sm text-gray-300">Rings</span>
      </div>

      {/* Bubble 6: Matrix Code Pattern - Green */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <clipPath id="matrixClip">
              <circle cx="60" cy="60" r="50" />
            </clipPath>
            <linearGradient id="matrixGradient" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="#10B981" opacity="0.1" />
              <stop offset="50%" stopColor="#10B981" opacity="0.8" />
              <stop offset="100%" stopColor="#10B981" opacity="0.2" />
            </linearGradient>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="#000000" opacity="0.8" />
          
          <g clipPath="url(#matrixClip)">
            {/* Column 1 */}
            <text x="18" y="0" fill="url(#matrixGradient)" fontSize="9" fontFamily="monospace" opacity="0.9">
              <tspan x="18" dy="0">1</tspan>
              <tspan x="18" dy="11">0</tspan>
              <tspan x="18" dy="11">1</tspan>
              <tspan x="18" dy="11">1</tspan>
              <tspan x="18" dy="11">0</tspan>
              <tspan x="18" dy="11">1</tspan>
              <tspan x="18" dy="11">0</tspan>
              <tspan x="18" dy="11">1</tspan>
              <tspan x="18" dy="11">1</tspan>
              <tspan x="18" dy="11">0</tspan>
              <tspan x="18" dy="11">1</tspan>
              <animate attributeName="y" values="0;120;0;120;0" dur="6s" repeatCount="indefinite" begin="0.5s" />
            </text>
            
            {/* Column 2 */}
            <text x="28" y="-30" fill="#10B981" fontSize="9" fontFamily="monospace" opacity="0.7">
              <tspan x="28" dy="0">0</tspan>
              <tspan x="28" dy="11">1</tspan>
              <tspan x="28" dy="11">0</tspan>
              <tspan x="28" dy="11">1</tspan>
              <tspan x="28" dy="11">1</tspan>
              <tspan x="28" dy="11">0</tspan>
              <tspan x="28" dy="11">1</tspan>
              <tspan x="28" dy="11">0</tspan>
              <tspan x="28" dy="11">1</tspan>
              <tspan x="28" dy="11">1</tspan>
              <tspan x="28" dy="11">0</tspan>
              <animate attributeName="y" values="-30;120;-30" dur="4.5s" repeatCount="indefinite" begin="1.2s" />
            </text>
            
            {/* Column 3 */}
            <text x="38" y="-50" fill="#34D399" fontSize="9" fontFamily="monospace" opacity="0.8">
              <tspan x="38" dy="0">1</tspan>
              <tspan x="38" dy="11">1</tspan>
              <tspan x="38" dy="11">0</tspan>
              <tspan x="38" dy="11">0</tspan>
              <tspan x="38" dy="11">1</tspan>
              <tspan x="38" dy="11">0</tspan>
              <tspan x="38" dy="11">1</tspan>
              <tspan x="38" dy="11">1</tspan>
              <tspan x="38" dy="11">0</tspan>
              <tspan x="38" dy="11">1</tspan>
              <tspan x="38" dy="11">1</tspan>
              <animate attributeName="y" values="-50;120;-50;120;-50" dur="7s" repeatCount="indefinite" begin="0s" />
            </text>
            
            {/* Column 4 */}
            <text x="48" y="-20" fill="#10B981" fontSize="9" fontFamily="monospace" opacity="0.6">
              <tspan x="48" dy="0">0</tspan>
              <tspan x="48" dy="11">0</tspan>
              <tspan x="48" dy="11">1</tspan>
              <tspan x="48" dy="11">0</tspan>
              <tspan x="48" dy="11">1</tspan>
              <tspan x="48" dy="11">1</tspan>
              <tspan x="48" dy="11">0</tspan>
              <tspan x="48" dy="11">1</tspan>
              <tspan x="48" dy="11">0</tspan>
              <tspan x="48" dy="11">1</tspan>
              <tspan x="48" dy="11">0</tspan>
              <animate attributeName="y" values="-20;120;-20" dur="3.8s" repeatCount="indefinite" begin="2.1s" />
            </text>
            
            {/* Column 5 */}
            <text x="58" y="-40" fill="#34D399" fontSize="9" fontFamily="monospace" opacity="0.7">
              <tspan x="58" dy="0">1</tspan>
              <tspan x="58" dy="11">0</tspan>
              <tspan x="58" dy="11">1</tspan>
              <tspan x="58" dy="11">1</tspan>
              <tspan x="58" dy="11">0</tspan>
              <tspan x="58" dy="11">0</tspan>
              <tspan x="58" dy="11">1</tspan>
              <tspan x="58" dy="11">0</tspan>
              <tspan x="58" dy="11">1</tspan>
              <tspan x="58" dy="11">1</tspan>
              <tspan x="58" dy="11">1</tspan>
              <animate attributeName="y" values="-40;120;-40;-40;120;-40" dur="8s" repeatCount="indefinite" begin="0.8s" />
            </text>
            
            {/* Column 6 */}
            <text x="68" y="-15" fill="#10B981" fontSize="9" fontFamily="monospace" opacity="0.8">
              <tspan x="68" dy="0">1</tspan>
              <tspan x="68" dy="11">1</tspan>
              <tspan x="68" dy="11">0</tspan>
              <tspan x="68" dy="11">1</tspan>
              <tspan x="68" dy="11">0</tspan>
              <tspan x="68" dy="11">1</tspan>
              <tspan x="68" dy="11">1</tspan>
              <tspan x="68" dy="11">0</tspan>
              <tspan x="68" dy="11">0</tspan>
              <tspan x="68" dy="11">1</tspan>
              <tspan x="68" dy="11">0</tspan>
              <animate attributeName="y" values="-15;120;-15" dur="5.2s" repeatCount="indefinite" begin="1.5s" />
            </text>
            
            {/* Column 7 */}
            <text x="78" y="-35" fill="#34D399" fontSize="9" fontFamily="monospace" opacity="0.6">
              <tspan x="78" dy="0">0</tspan>
              <tspan x="78" dy="11">1</tspan>
              <tspan x="78" dy="11">1</tspan>
              <tspan x="78" dy="11">0</tspan>
              <tspan x="78" dy="11">0</tspan>
              <tspan x="78" dy="11">1</tspan>
              <tspan x="78" dy="11">0</tspan>
              <tspan x="78" dy="11">1</tspan>
              <tspan x="78" dy="11">1</tspan>
              <tspan x="78" dy="11">0</tspan>
              <tspan x="78" dy="11">1</tspan>
              <animate attributeName="y" values="-35;120;-35;120;-35" dur="6.5s" repeatCount="indefinite" begin="0.3s" />
            </text>
            
            {/* Column 8 */}
            <text x="88" y="-25" fill="#10B981" fontSize="9" fontFamily="monospace" opacity="0.7">
              <tspan x="88" dy="0">1</tspan>
              <tspan x="88" dy="11">0</tspan>
              <tspan x="88" dy="11">0</tspan>
              <tspan x="88" dy="11">1</tspan>
              <tspan x="88" dy="11">1</tspan>
              <tspan x="88" dy="11">0</tspan>
              <tspan x="88" dy="11">1</tspan>
              <tspan x="88" dy="11">0</tspan>
              <tspan x="88" dy="11">1</tspan>
              <tspan x="88" dy="11">0</tspan>
              <tspan x="88" dy="11">1</tspan>
              <animate attributeName="y" values="-25;120;-25" dur="4.2s" repeatCount="indefinite" begin="1.8s" />
            </text>
            
            {/* Column 9 */}
            <text x="98" y="-45" fill="#34D399" fontSize="9" fontFamily="monospace" opacity="0.8">
              <tspan x="98" dy="0">0</tspan>
              <tspan x="98" dy="11">1</tspan>
              <tspan x="98" dy="11">0</tspan>
              <tspan x="98" dy="11">0</tspan>
              <tspan x="98" dy="11">1</tspan>
              <tspan x="98" dy="11">1</tspan>
              <tspan x="98" dy="11">0</tspan>
              <tspan x="98" dy="11">1</tspan>
              <tspan x="98" dy="11">0</tspan>
              <tspan x="98" dy="11">1</tspan>
              <tspan x="98" dy="11">1</tspan>
              <animate attributeName="y" values="-45;120;-45;-45;120;-45" dur="7.5s" repeatCount="indefinite" begin="0.6s" />
            </text>
          </g>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="#10B981" strokeWidth="2" opacity="0.5" />
        </svg>
        <span className="text-sm text-gray-300">Matrix</span>
      </div>

      {/* Bubble 7: Orbiting Particles - Orange */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <radialGradient id="orangeGradient">
              <stop offset="0%" stopColor="#F59E0B" />
              <stop offset="100%" stopColor="#D97706" />
            </radialGradient>
            <clipPath id="orbitClip">
              <circle cx="60" cy="60" r="50" />
            </clipPath>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="url(#orangeGradient)" opacity="0.3" />
          
          <g clipPath="url(#orbitClip)">
            {/* Inner orbit particles */}
            <g transform-origin="60 60">
              <circle cx="60" cy="35" r="2" fill="#FCD34D">
                <animateTransform attributeName="transform" type="rotate" from="0 60 60" to="360 60 60" dur="8s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="60" cy="38" r="1.5" fill="#F59E0B">
                <animateTransform attributeName="transform" type="rotate" from="90 60 60" to="450 60 60" dur="9.5s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="60" cy="36" r="2.5" fill="#D97706">
                <animateTransform attributeName="transform" type="rotate" from="180 60 60" to="540 60 60" dur="8.8s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="60" cy="33" r="1.8" fill="#FCD34D">
                <animateTransform attributeName="transform" type="rotate" from="270 60 60" to="630 60 60" dur="10s" repeatCount="indefinite" />
              </circle>
            </g>
            
            {/* Middle orbit particles */}
            <g transform-origin="60 60">
              <circle cx="60" cy="25" r="3" fill="#F59E0B">
                <animateTransform attributeName="transform" type="rotate" from="0 60 60" to="360 60 60" dur="12s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="60" cy="30" r="2.5" fill="#FCD34D">
                <animateTransform attributeName="transform" type="rotate" from="120 60 60" to="480 60 60" dur="14s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="60" cy="28" r="2" fill="#D97706">
                <animateTransform attributeName="transform" type="rotate" from="240 60 60" to="600 60 60" dur="11s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="60" cy="26" r="2.2" fill="#F59E0B">
                <animateTransform attributeName="transform" type="rotate" from="60 60 60" to="420 60 60" dur="13s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="60" cy="32" r="1.8" fill="#FCD34D">
                <animateTransform attributeName="transform" type="rotate" from="300 60 60" to="660 60 60" dur="12.5s" repeatCount="indefinite" />
              </circle>
            </g>
            
            {/* Outer orbit particles */}
            <g transform-origin="60 60">
              <circle cx="80" cy="60" r="3" fill="#D97706">
                <animateTransform attributeName="transform" type="rotate" from="90 60 60" to="450 60 60" dur="16s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="85" cy="60" r="2" fill="#FCD34D">
                <animateTransform attributeName="transform" type="rotate" from="270 60 60" to="630 60 60" dur="14.5s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="83" cy="60" r="2.5" fill="#F59E0B">
                <animateTransform attributeName="transform" type="rotate" from="30 60 60" to="390 60 60" dur="18s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="82" cy="60" r="1.5" fill="#D97706">
                <animateTransform attributeName="transform" type="rotate" from="150 60 60" to="510 60 60" dur="15s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="84" cy="60" r="2.2" fill="#FCD34D">
                <animateTransform attributeName="transform" type="rotate" from="210 60 60" to="570 60 60" dur="17s" repeatCount="indefinite" />
              </circle>
            </g>
            
            <g transform-origin="60 60">
              <circle cx="81" cy="60" r="1.8" fill="#F59E0B">
                <animateTransform attributeName="transform" type="rotate" from="330 60 60" to="690 60 60" dur="15.5s" repeatCount="indefinite" />
              </circle>
            </g>
            
            {/* Flying star 1 - diagonal from bottom-left to top-right */}
            <g transform="rotate(-35 60 60)">
              <path d="M 0,80 L 2,78 L 4,80 L 3,77 L 5,75 L 2,75 L 1,72 L 0,75 L -3,75 L -1,77 Z" fill="#FFF" opacity="0">
                <animate attributeName="opacity" 
                  values="0;0;0;0;0;0;0;0;0;0.8;1;0.8;0;0;0;0;0;0" 
                  dur="18s" repeatCount="indefinite" />
                <animateTransform attributeName="transform" type="translate" 
                  values="0 0; 0 0; 0 0; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 120 -160; 0 0" 
                  dur="18s" repeatCount="indefinite" />
              </path>
            </g>
            
            {/* Flying star 2 - horizontal from left to right */}
            <g transform="rotate(15 60 60)">
              <path d="M 0,45 L 2.5,43 L 5,45 L 4,41.5 L 6.5,39 L 3,39 L 1.5,35 L 0,39 L -3.5,39 L -1,41.5 Z" fill="#FCD34D" opacity="0">
                <animate attributeName="opacity" 
                  values="0;0;0;0;0;0.7;1;0.7;0;0;0;0;0;0;0;0" 
                  dur="15s" repeatCount="indefinite" />
                <animateTransform attributeName="transform" type="translate" 
                  values="-20 0; -20 0; -20 0; -20 0; -20 0; 140 0; 140 0; 140 0; 140 0; 140 0; 140 0; 140 0; 140 0; 140 0; 140 0; 140 0; -20 0" 
                  dur="15s" repeatCount="indefinite" />
              </path>
            </g>
            
            {/* Flying star 3 - diagonal from top-left to bottom-right */}
            <g transform="rotate(50 60 60)">
              <path d="M 10,15 L 11.5,13.5 L 13,15 L 12.5,12.5 L 14.5,11 L 12,11 L 11,8.5 L 10,11 L 7.5,11 L 9.5,12.5 Z" fill="#F59E0B" opacity="0">
                <animate attributeName="opacity" 
                  values="0;0;0;0;0;0;0;0;0;0.6;0.9;0.6;0;0;0;0;0;0;0;0" 
                  dur="20s" repeatCount="indefinite" />
                <animateTransform attributeName="transform" type="translate" 
                  values="0 0; 0 0; 0 0; 0 0; 0 0; 0 0; 0 0; 0 0; 0 0; 100 150; 100 150; 100 150; 100 150; 100 150; 100 150; 100 150; 100 150; 100 150; 100 150; 0 0" 
                  dur="20s" repeatCount="indefinite" />
              </path>
            </g>
          </g>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="#F59E0B" strokeWidth="2" opacity="0.5" />
        </svg>
        <span className="text-sm text-gray-300">Orbit</span>
      </div>

      {/* Bubble 8: Morphing Shapes - Multi-color Gradient */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          <defs>
            <linearGradient id="multiGradient" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#60A5FA">
                <animate attributeName="stop-color" 
                  values="#60A5FA;#8B5CF6;#EC4899;#F59E0B;#60A5FA" 
                  dur="6s" repeatCount="indefinite" />
              </stop>
              <stop offset="50%" stopColor="#8B5CF6">
                <animate attributeName="stop-color" 
                  values="#8B5CF6;#EC4899;#F59E0B;#60A5FA;#8B5CF6" 
                  dur="6s" repeatCount="indefinite" />
              </stop>
              <stop offset="100%" stopColor="#EC4899">
                <animate attributeName="stop-color" 
                  values="#EC4899;#F59E0B;#60A5FA;#8B5CF6;#EC4899" 
                  dur="6s" repeatCount="indefinite" />
              </stop>
            </linearGradient>
          </defs>
          
          <circle cx="60" cy="60" r="50" fill="url(#multiGradient)" opacity="0.4" />
          
          <path fill="url(#multiGradient)" opacity="0.8">
            <animate attributeName="d" 
              values="M 50,35 Q 68,32 78,45 Q 85,55 82,68 Q 75,80 62,84 Q 48,86 40,75 Q 35,63 38,52 Q 42,38 50,35 Z;
                      M 55,40 Q 65,33 80,42 Q 88,52 84,65 Q 78,76 66,82 Q 52,87 42,78 Q 33,68 35,55 Q 38,42 55,40 Z;
                      M 45,38 Q 63,30 75,43 Q 85,54 80,67 Q 74,80 60,86 Q 46,84 38,72 Q 32,60 36,48 Q 38,35 45,38 Z;
                      M 52,33 Q 70,36 82,50 Q 87,62 78,74 Q 68,84 56,82 Q 42,78 38,66 Q 33,52 40,42 Q 45,32 52,33 Z;
                      M 50,35 Q 68,32 78,45 Q 85,55 82,68 Q 75,80 62,84 Q 48,86 40,75 Q 35,63 38,52 Q 42,38 50,35 Z"
              dur="5s" repeatCount="indefinite" />
          </path>
          
          <circle cx="60" cy="60" r="50" fill="none" stroke="url(#multiGradient)" strokeWidth="2" opacity="0.6">
            <animate attributeName="stroke-dasharray" values="0,314;314,314;0,314" dur="5s" repeatCount="indefinite" />
          </circle>
        </svg>
        <span className="text-sm text-gray-300">Morph</span>
      </div>

      {/* Bubble 9: Taskbar Icon - Static */}
      <div className="flex flex-col items-center gap-2">
        <svg width="120" height="120" viewBox="0 0 120 120">
          {/* Background circle */}
          <circle cx="60" cy="60" r="50" fill="#000000" opacity="0.8" />
          
          {/* Taskbar icon scaled up */}
          <g transform="translate(60, 60) scale(1.75)">
            {/* Central circle with crown */}
            <circle
              cx="0"
              cy="0"
              r="14"
              fill="url(#iconCoreGradient)"
            />
            
            {/* Very simple crown */}
            <g>
              {/* Crown base */}
              <path
                d="M -5 2 L -6 4 L 6 4 L 5 2 Z"
                fill="url(#iconCrownGradient)"
              />
              
              {/* Three peaks - simplified */}
              <path
                d="M -5 2 L -4 -4 L -3 2 M -1 2 L 0 -5 L 1 2 M 3 2 L 4 -4 L 5 2"
                fill="url(#iconCrownGradient)"
              />
            </g>
          </g>
          
          {/* Outer border */}
          <circle cx="60" cy="60" r="50" fill="none" stroke="#F59E0B" strokeWidth="2" opacity="0.5" />
          
          {/* Gradients */}
          <defs>
            <radialGradient id="iconCoreGradient">
              <stop offset="0%" stopColor="#FCD34D" />
              <stop offset="50%" stopColor="#F59E0B" />
              <stop offset="100%" stopColor="#D97706" />
            </radialGradient>
            
            <linearGradient id="iconCrownGradient" x1="0%" y1="0%" x2="0%" y2="100%">
              <stop offset="0%" stopColor="#CD7F32" />
              <stop offset="50%" stopColor="#A0522D" />
              <stop offset="100%" stopColor="#8B4513" />
            </linearGradient>
          </defs>
        </svg>
        <span className="text-sm text-gray-300">Icon</span>
      </div>
    </div>
  );
}