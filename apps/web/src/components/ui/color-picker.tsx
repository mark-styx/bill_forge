'use client';

import { useState, useCallback, useRef, useEffect } from 'react';
import { cn } from '@/lib/utils';
import { Check, Pipette, Sparkles, Copy, RotateCcw } from 'lucide-react';
import { toast } from 'sonner';

interface ColorPickerProps {
  value: string; // HSL string like "210 100% 50%"
  onChange: (value: string) => void;
  label?: string;
  presets?: { name: string; value: string }[];
  className?: string;
  showGradientPreview?: boolean;
  gradientWith?: string; // Second color for gradient preview
}

interface GradientPickerProps {
  fromColor: string;
  toColor: string;
  viaColor?: string;
  angle?: number;
  onChange: (gradient: { from: string; to: string; via?: string; angle: number }) => void;
  className?: string;
}

// Bright, vibrant preset colors - expanded
const defaultPresets = [
  // Blues
  { name: 'Blue', value: '210 100% 50%' },
  { name: 'Sky', value: '200 100% 50%' },
  { name: 'Cyan', value: '190 100% 45%' },
  { name: 'Azure', value: '217 91% 60%' },
  // Greens
  { name: 'Teal', value: '170 80% 40%' },
  { name: 'Emerald', value: '160 84% 39%' },
  { name: 'Green', value: '142 76% 45%' },
  { name: 'Lime', value: '84 85% 45%' },
  // Warm
  { name: 'Yellow', value: '50 100% 50%' },
  { name: 'Amber', value: '38 92% 50%' },
  { name: 'Orange', value: '25 95% 53%' },
  { name: 'Coral', value: '10 90% 60%' },
  // Pinks
  { name: 'Rose', value: '350 90% 60%' },
  { name: 'Pink', value: '330 85% 60%' },
  { name: 'Fuchsia', value: '300 80% 55%' },
  // Purples
  { name: 'Purple', value: '270 75% 55%' },
  { name: 'Violet', value: '260 70% 55%' },
  { name: 'Indigo', value: '245 75% 55%' },
  // Neutral
  { name: 'Slate', value: '215 28% 45%' },
  { name: 'Gray', value: '220 14% 46%' },
];

// Gradient presets
const gradientPresets = [
  { name: 'Ocean', from: '210 100% 50%', to: '190 95% 45%' },
  { name: 'Sunset', from: '25 95% 55%', to: '340 80% 55%' },
  { name: 'Aurora', from: '150 80% 50%', to: '280 80% 55%', via: '190 100% 50%' },
  { name: 'Cosmic', from: '280 85% 50%', to: '195 100% 50%', via: '230 85% 55%' },
  { name: 'Forest', from: '160 84% 39%', to: '84 85% 45%' },
  { name: 'Fire', from: '0 85% 58%', to: '38 92% 50%' },
  { name: 'Neon', from: '180 100% 50%', to: '300 100% 55%' },
  { name: 'Berry', from: '300 80% 55%', to: '350 89% 60%' },
];

function parseHSL(hsl: string): { h: number; s: number; l: number } {
  const parts = hsl.split(' ').map((p) => parseFloat(p.replace('%', '')));
  return { h: parts[0] || 0, s: parts[1] || 0, l: parts[2] || 0 };
}

function formatHSL(h: number, s: number, l: number): string {
  return `${Math.round(h)} ${Math.round(s)}% ${Math.round(l)}%`;
}

export function ColorPicker({
  value,
  onChange,
  label,
  presets = defaultPresets,
  className,
  showGradientPreview = false,
  gradientWith,
}: ColorPickerProps) {
  const { h, s, l } = parseHSL(value);
  const [hue, setHue] = useState(h);
  const [saturation, setSaturation] = useState(s);
  const [lightness, setLightness] = useState(l);
  const [isCustomMode, setIsCustomMode] = useState(false);

  const copyColor = useCallback(() => {
    navigator.clipboard.writeText(`hsl(${value})`);
    toast.success('Color copied to clipboard');
  }, [value]);

  const resetToDefault = useCallback(() => {
    handlePresetClick('210 100% 50%');
    toast.info('Reset to default blue');
  }, []);

  const handleHueChange = useCallback((newHue: number) => {
    setHue(newHue);
    onChange(formatHSL(newHue, saturation, lightness));
  }, [saturation, lightness, onChange]);

  const handleSaturationChange = useCallback((newSat: number) => {
    setSaturation(newSat);
    onChange(formatHSL(hue, newSat, lightness));
  }, [hue, lightness, onChange]);

  const handleLightnessChange = useCallback((newLight: number) => {
    setLightness(newLight);
    onChange(formatHSL(hue, saturation, newLight));
  }, [hue, saturation, onChange]);

  const handlePresetClick = useCallback((presetValue: string) => {
    const parsed = parseHSL(presetValue);
    setHue(parsed.h);
    setSaturation(parsed.s);
    setLightness(parsed.l);
    onChange(presetValue);
    setIsCustomMode(false);
  }, [onChange]);

  // Check if current value matches a preset
  const matchingPreset = presets.find((p) => {
    const parsed = parseHSL(p.value);
    return Math.abs(parsed.h - hue) < 5 &&
           Math.abs(parsed.s - saturation) < 5 &&
           Math.abs(parsed.l - lightness) < 5;
  });

  return (
    <div className={cn('space-y-4', className)}>
      {label && (
        <label className="block text-sm font-medium text-foreground">{label}</label>
      )}

      {/* Preset Colors Grid */}
      <div className="space-y-2">
        <p className="text-xs text-muted-foreground font-medium uppercase tracking-wide">
          Presets
        </p>
        <div className="grid grid-cols-8 gap-2">
          {presets.map((preset) => {
            const isSelected = matchingPreset?.value === preset.value && !isCustomMode;
            return (
              <button
                key={preset.value}
                type="button"
                onClick={() => handlePresetClick(preset.value)}
                className={cn(
                  'w-8 h-8 rounded-lg transition-all relative group',
                  'ring-offset-background focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2',
                  isSelected && 'ring-2 ring-primary ring-offset-2'
                )}
                style={{ backgroundColor: `hsl(${preset.value})` }}
                title={preset.name}
              >
                {isSelected && (
                  <Check className="w-4 h-4 text-white absolute inset-0 m-auto drop-shadow-md" />
                )}
                <span className="absolute -bottom-5 left-1/2 -translate-x-1/2 text-[10px] text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap">
                  {preset.name}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Custom Color Section */}
      <div className="space-y-3 pt-3 border-t border-border">
        <button
          type="button"
          onClick={() => setIsCustomMode(!isCustomMode)}
          className={cn(
            'flex items-center gap-2 text-sm font-medium transition-colors',
            isCustomMode ? 'text-primary' : 'text-muted-foreground hover:text-foreground'
          )}
        >
          <Pipette className="w-4 h-4" />
          Custom Color
        </button>

        {isCustomMode && (
          <div className="space-y-4 animate-scale-in p-4 bg-secondary/50 rounded-xl">
            {/* Preview */}
            <div className="flex items-center gap-4">
              <div
                className="w-14 h-14 rounded-xl shadow-soft border border-border"
                style={{ backgroundColor: `hsl(${formatHSL(hue, saturation, lightness)})` }}
              />
              <div className="flex-1 text-sm">
                <p className="font-medium text-foreground">Preview</p>
                <p className="text-xs text-muted-foreground font-mono mt-0.5">
                  hsl({Math.round(hue)}, {Math.round(saturation)}%, {Math.round(lightness)}%)
                </p>
              </div>
            </div>

            {/* Hue Slider */}
            <div className="space-y-1.5">
              <div className="flex justify-between text-xs">
                <span className="text-muted-foreground">Hue</span>
                <span className="text-foreground font-medium">{Math.round(hue)}°</span>
              </div>
              <input
                type="range"
                min="0"
                max="360"
                value={hue}
                onChange={(e) => handleHueChange(Number(e.target.value))}
                className="w-full h-3 rounded-full appearance-none cursor-pointer"
                style={{
                  background: `linear-gradient(to right,
                    hsl(0, 100%, 50%),
                    hsl(60, 100%, 50%),
                    hsl(120, 100%, 50%),
                    hsl(180, 100%, 50%),
                    hsl(240, 100%, 50%),
                    hsl(300, 100%, 50%),
                    hsl(360, 100%, 50%)
                  )`,
                }}
              />
            </div>

            {/* Saturation Slider */}
            <div className="space-y-1.5">
              <div className="flex justify-between text-xs">
                <span className="text-muted-foreground">Saturation</span>
                <span className="text-foreground font-medium">{Math.round(saturation)}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="100"
                value={saturation}
                onChange={(e) => handleSaturationChange(Number(e.target.value))}
                className="w-full h-3 rounded-full appearance-none cursor-pointer"
                style={{
                  background: `linear-gradient(to right,
                    hsl(${hue}, 0%, ${lightness}%),
                    hsl(${hue}, 100%, ${lightness}%)
                  )`,
                }}
              />
            </div>

            {/* Lightness Slider */}
            <div className="space-y-1.5">
              <div className="flex justify-between text-xs">
                <span className="text-muted-foreground">Lightness</span>
                <span className="text-foreground font-medium">{Math.round(lightness)}%</span>
              </div>
              <input
                type="range"
                min="20"
                max="70"
                value={lightness}
                onChange={(e) => handleLightnessChange(Number(e.target.value))}
                className="w-full h-3 rounded-full appearance-none cursor-pointer"
                style={{
                  background: `linear-gradient(to right,
                    hsl(${hue}, ${saturation}%, 20%),
                    hsl(${hue}, ${saturation}%, 50%),
                    hsl(${hue}, ${saturation}%, 70%)
                  )`,
                }}
              />
            </div>
          </div>
        )}
      </div>

      {/* Current Color Display */}
      <div className="flex items-center gap-3 p-3 bg-secondary/50 rounded-lg">
        <div className="relative">
          <div
            className="w-10 h-10 rounded-lg shadow-sm border border-border"
            style={{ backgroundColor: `hsl(${value})` }}
          />
          {showGradientPreview && gradientWith && (
            <div
              className="absolute -bottom-1 -right-1 w-5 h-5 rounded-md shadow-sm border border-border"
              style={{ backgroundColor: `hsl(${gradientWith})` }}
            />
          )}
        </div>
        <div className="flex-1">
          <p className="text-sm font-medium text-foreground">
            {matchingPreset && !isCustomMode ? matchingPreset.name : 'Custom Color'}
          </p>
          <p className="text-xs text-muted-foreground font-mono">
            hsl({Math.round(hue)}, {Math.round(saturation)}%, {Math.round(lightness)}%)
          </p>
        </div>
        <div className="flex gap-1">
          <button
            type="button"
            onClick={copyColor}
            className="p-1.5 rounded-md hover:bg-secondary transition-colors text-muted-foreground hover:text-foreground"
            title="Copy color"
          >
            <Copy className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={resetToDefault}
            className="p-1.5 rounded-md hover:bg-secondary transition-colors text-muted-foreground hover:text-foreground"
            title="Reset to default"
          >
            <RotateCcw className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Gradient Preview */}
      {showGradientPreview && gradientWith && (
        <div className="mt-4 pt-4 border-t border-border">
          <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
            Gradient Preview
          </p>
          <div
            className="h-12 rounded-lg shadow-inner"
            style={{
              background: `linear-gradient(135deg, hsl(${value}), hsl(${gradientWith}))`,
            }}
          />
        </div>
      )}
    </div>
  );
}

// Gradient Picker Component
export function GradientPicker({
  fromColor,
  toColor,
  viaColor,
  angle = 135,
  onChange,
  className,
}: GradientPickerProps) {
  const [from, setFrom] = useState(fromColor);
  const [to, setTo] = useState(toColor);
  const [via, setVia] = useState(viaColor || '');
  const [gradientAngle, setGradientAngle] = useState(angle);
  const [useViaColor, setUseViaColor] = useState(!!viaColor);

  const updateGradient = useCallback(() => {
    onChange({
      from,
      to,
      via: useViaColor && via ? via : undefined,
      angle: gradientAngle,
    });
  }, [from, to, via, gradientAngle, useViaColor, onChange]);

  useEffect(() => {
    updateGradient();
  }, [from, to, via, gradientAngle, useViaColor]);

  const getGradientCSS = () => {
    if (useViaColor && via) {
      return `linear-gradient(${gradientAngle}deg, hsl(${from}), hsl(${via}), hsl(${to}))`;
    }
    return `linear-gradient(${gradientAngle}deg, hsl(${from}), hsl(${to}))`;
  };

  const copyGradientCSS = () => {
    navigator.clipboard.writeText(getGradientCSS());
    toast.success('Gradient CSS copied');
  };

  return (
    <div className={cn('space-y-4', className)}>
      {/* Gradient Preview */}
      <div className="relative">
        <div
          className="h-24 rounded-xl shadow-inner border border-border"
          style={{ background: getGradientCSS() }}
        />
        <button
          onClick={copyGradientCSS}
          className="absolute top-2 right-2 p-1.5 rounded-md bg-black/20 text-white hover:bg-black/30 transition-colors"
          title="Copy CSS"
        >
          <Copy className="w-4 h-4" />
        </button>
      </div>

      {/* Gradient Presets */}
      <div>
        <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
          Presets
        </p>
        <div className="grid grid-cols-4 gap-2">
          {gradientPresets.map((preset) => {
            const isSelected = from === preset.from && to === preset.to;
            const presetGradient = preset.via
              ? `linear-gradient(135deg, hsl(${preset.from}), hsl(${preset.via}), hsl(${preset.to}))`
              : `linear-gradient(135deg, hsl(${preset.from}), hsl(${preset.to}))`;

            return (
              <button
                key={preset.name}
                onClick={() => {
                  setFrom(preset.from);
                  setTo(preset.to);
                  if (preset.via) {
                    setVia(preset.via);
                    setUseViaColor(true);
                  } else {
                    setUseViaColor(false);
                  }
                }}
                className={cn(
                  'h-10 rounded-lg transition-all',
                  isSelected && 'ring-2 ring-primary ring-offset-2'
                )}
                style={{ background: presetGradient }}
                title={preset.name}
              >
                {isSelected && (
                  <Check className="w-4 h-4 text-white m-auto drop-shadow" />
                )}
              </button>
            );
          })}
        </div>
      </div>

      {/* Angle Control */}
      <div>
        <div className="flex justify-between text-xs mb-1">
          <span className="text-muted-foreground">Angle</span>
          <span className="text-foreground font-medium">{gradientAngle}°</span>
        </div>
        <input
          type="range"
          min="0"
          max="360"
          value={gradientAngle}
          onChange={(e) => setGradientAngle(Number(e.target.value))}
          className="w-full h-2 rounded-full appearance-none cursor-pointer bg-secondary"
        />
        <div className="flex justify-between text-[10px] text-muted-foreground mt-1">
          <span>0°</span>
          <span>90°</span>
          <span>180°</span>
          <span>270°</span>
          <span>360°</span>
        </div>
      </div>

      {/* Via Color Toggle */}
      <div className="flex items-center justify-between p-3 bg-secondary/50 rounded-lg">
        <div className="flex items-center gap-2">
          <Sparkles className="w-4 h-4 text-muted-foreground" />
          <span className="text-sm font-medium text-foreground">Three-color gradient</span>
        </div>
        <button
          onClick={() => setUseViaColor(!useViaColor)}
          className={cn(
            'relative w-11 h-6 rounded-full transition-colors',
            useViaColor ? 'bg-primary' : 'bg-secondary'
          )}
        >
          <span
            className={cn(
              'absolute top-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform',
              useViaColor ? 'translate-x-5 left-0.5' : 'left-0.5'
            )}
          />
        </button>
      </div>

      {/* Color Inputs */}
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label className="text-xs font-medium text-muted-foreground mb-1.5 block">
            From
          </label>
          <div className="flex items-center gap-2">
            <div
              className="w-8 h-8 rounded-lg border border-border flex-shrink-0"
              style={{ backgroundColor: `hsl(${from})` }}
            />
            <input
              type="text"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              className="input text-xs font-mono"
              placeholder="210 100% 50%"
            />
          </div>
        </div>
        <div>
          <label className="text-xs font-medium text-muted-foreground mb-1.5 block">
            To
          </label>
          <div className="flex items-center gap-2">
            <div
              className="w-8 h-8 rounded-lg border border-border flex-shrink-0"
              style={{ backgroundColor: `hsl(${to})` }}
            />
            <input
              type="text"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              className="input text-xs font-mono"
              placeholder="190 95% 45%"
            />
          </div>
        </div>
      </div>

      {/* Via Color Input */}
      {useViaColor && (
        <div className="animate-scale-in">
          <label className="text-xs font-medium text-muted-foreground mb-1.5 block">
            Via (Middle)
          </label>
          <div className="flex items-center gap-2">
            <div
              className="w-8 h-8 rounded-lg border border-border flex-shrink-0"
              style={{ backgroundColor: `hsl(${via || '220 100% 55%'})` }}
            />
            <input
              type="text"
              value={via}
              onChange={(e) => setVia(e.target.value)}
              className="input text-xs font-mono"
              placeholder="220 100% 55%"
            />
          </div>
        </div>
      )}

      {/* CSS Output */}
      <div className="p-3 bg-secondary/50 rounded-lg">
        <p className="text-xs font-medium text-muted-foreground mb-1">CSS</p>
        <code className="text-xs text-foreground font-mono break-all">
          {getGradientCSS()}
        </code>
      </div>
    </div>
  );
}

// Simplified inline color swatch for forms
interface ColorSwatchProps {
  value: string;
  onChange: (value: string) => void;
  colors?: { name: string; value: string }[];
  className?: string;
}

export function ColorSwatch({
  value,
  onChange,
  colors = defaultPresets.slice(0, 8),
  className,
}: ColorSwatchProps) {
  return (
    <div className={cn('flex gap-1.5 flex-wrap', className)}>
      {colors.map((color) => {
        const isSelected = value === color.value;
        return (
          <button
            key={color.value}
            type="button"
            onClick={() => onChange(color.value)}
            className={cn(
              'w-7 h-7 rounded-md transition-all',
              'focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1',
              isSelected && 'ring-2 ring-primary ring-offset-1'
            )}
            style={{ backgroundColor: `hsl(${color.value})` }}
            title={color.name}
          >
            {isSelected && (
              <Check className="w-3.5 h-3.5 text-white m-auto drop-shadow" />
            )}
          </button>
        );
      })}
    </div>
  );
}
