// Render the public property surface: direct enums/units and checked results.
import { definitionSyntax } from 'css-tree';
import { atomAdopted } from './adoption.mjs';
import { comment } from './documentation.mjs';
const globals = ['Inherit', 'Initial', 'Unset', 'Revert', 'RevertLayer'];
const globalCss = new Set(['inherit', 'initial', 'unset', 'revert', 'revert-layer']);
const primitives = new Map([
  ['length', ['Length','length_css']], ['percentage',['Percentage','percentage_css']],
  ['color',['Color','color_css']], ['angle',['Angle','angle_css']], ['time',['Time','time_css']],
]);
const reserved = new Set(['Value','Css','Property','Checked','Component','CheckedComponent','SerializedValue','Sequence','Composition','Inherit','Initial','Unset','Revert','RevertLayer','Absent','Some','Option','Length','Percentage','Color','ColorCss','Angle','Time','Int','Float','Number','String','Text','Binary','Atom','Bool','Term','Dynamic','Never','Unit','List','Map','Set','Iterator','Task']);
const unique = values => [...new Set(values)];
export function renderProperty(property, definitions, adoptedAtoms, context) {
  const {identifier,typeName,atomicValues,repeatedValues,groupedValues,documentation,propertyFacts,keywordModule,canonicalKeyword} = context;
  const name=typeName(property.name), docs=documentation.properties[property.name];
  const grammar=definitionSyntax.parse(property.syntax);
  const supported=values=>values.filter(atom=>adoptedAtoms===null || atomAdopted(property.name,atom,adoptedAtoms));
  const atoms=unique(supported(atomicValues(grammar,definitions,new Set(),property.name)).map(JSON.stringify)).map(s=>JSON.parse(s));
  let repeated=repeatedValues(grammar,definitions,new Set(),property.name);
  if(repeated){repeated={...repeated,atoms:supported(repeated.atoms)};if(!repeated.atoms.length)repeated=null;}
  let grouped=groupedValues(grammar,definitions,new Set(),property.name);
  if(grouped){grouped={...grouped,groups:grouped.groups.map(g=>({...g,atoms:supported(g.atoms)}))};if(grouped.groups.some(g=>!g.atoms.length))grouped=null;}
  const keywords=unique([...atoms,...(repeated?.atoms??[]),...(grouped?.groups.flatMap(g=>g.atoms)??[])].filter(a=>a.kind==='keyword'&&!globalCss.has(a.name)).map(a=>a.name)).sort();
  const reservedHere=new Set([...reserved,...(grouped?.groups.flatMap(g=>[`${typeName(g.name)}Part`,`Checked${typeName(g.name)}`])??[])]);
  const names=new Map(keywords.map(keyword=>{const n=typeName(keyword);return [keyword,reservedHere.has(n)?n+'Keyword':n];}));
  const imports=new Set(), declarations=[],functions=[];
  const types=new Set(),helpers=new Set(),payloads=[];
  if(!keywordModule)for(const [keyword,n] of names){
    if(docs?.values[keyword])declarations.push(comment(docs.values[keyword]+(docs.valueSources?.[keyword]?`\n\nAuthor guide: ${docs.valueSources[keyword]}`:'')));
    declarations.push(`pub type ${n} = Atom[${JSON.stringify(keyword)}].`);
  } else for(const [keyword,local] of names){const canonical=canonicalKeyword(keyword);imports.add(`import ${keywordModule}.${canonical}.{${canonical===local?canonical:`${canonical} as ${local}`}}.`);}
  const keywordTypes=values=>unique(values.filter(a=>a.kind==='keyword'&&!globalCss.has(a.name)).map(a=>names.get(a.name)));
  const sharedType=atom=>{
    const type=primitives.get(atom.name)?.[0]??(atom.name==='integer'?'Int':'Float');
    if(primitives.has(atom.name))types.add(type);
    if(atom.name==='color'){types.add('Transparent');types.add('CurrentColor');return 'Color | Transparent | CurrentColor';}
    return type;
  };
  const bounded=atom=>atom.kind==='primitive'&&(typeof atom.range?.min==='number'||typeof atom.range?.max==='number');
  const serialize=(atom)=>{
    const primitive=primitives.get(atom.name);
    if(primitive){imports.add('import std.css.Values.');return `std.css.Values.${primitive[1]}(value)`;}
    const type=atom.name==='integer'?'Int':'Float';imports.add(`import std.core.${type}.`);return `std.core.${type}.to_string(value)`;
  };
  const checked=(values,type,prefix='')=>{
    const constrained=values.filter(bounded);if(!constrained.length)return false;
    types.add('SerializedValue');helpers.add('serialize');
    declarations.push(`pub opaque type ${type} = SerializedValue.`);
    imports.add('import std.core.Option.{None as Absent, Some}.');imports.add('import type std.core.Option.');
    for(const atom of constrained){
      const parameter=sharedType(atom), conditions=[];
      const amount=primitives.has(atom.name)?`std.css.Values.${atom.name}_amount(value)`:'value';
      if(primitives.has(atom.name))imports.add('import std.css.Values.');
      if(typeof atom.range?.min==='number')conditions.push(`${amount} >= ${atom.range.min}`);
      if(typeof atom.range?.max==='number')conditions.push(`${amount} <= ${atom.range.max}`);
      functions.push(`${comment(`Checks the ${property.name} ${atom.name} range. Returns Some when ${conditions.map(c=>c.replaceAll(amount,'amount')).join(' and ')}; otherwise returns None.`)}\npub ${prefix+atom.name}(value: ${parameter}): Option[${type}] ->\n    case ${conditions.join(' and ')} {\n        true -> Some(${type}(serialize(${JSON.stringify(property.name)}, ${serialize(atom)})));\n        false -> Absent\n    }.`);
    }
    return true;
  };
  const choices=(values,checkedType)=>unique([...keywordTypes(values),...values.filter(a=>a.kind==='primitive'&&!bounded(a)).flatMap(atom=>sharedType(atom).split(' | ')),...(checkedType?[checkedType]:[])]);
  const physical = type => type.split(' | ').map(name => {
    const keyword = [...names].find(([,local])=>local===name)?.[0];
    if (keyword) return `Atom[${JSON.stringify(keyword)}]`;
    if (name.startsWith('Checked')) return 'std.css.Values.SerializedValue';
    if (['Length','Percentage','Color','Angle','Time','SerializedValue'].includes(name)) return 'std.css.Values.'+name;
    if (name==='Transparent') return 'Atom["transparent"]';
    if (name==='CurrentColor') return 'Atom["currentColor"]';
    return name;
  }).join(' | ');
  // Whole values use one checked payload only when a range or composition needs it.
  const hasRange=atoms.some(bounded);
  if(hasRange){
    // Emit the helper methods now; its single opaque declaration is emitted below.
    checked(atoms,'Checked');declarations.pop();payloads.push('SerializedValue');
  }
  if(repeated){
    const hasCheck=checked(repeated.atoms,'CheckedComponent','component_');
    const component=choices(repeated.atoms,hasCheck?'CheckedComponent':null).join(' | ');
    declarations.push(`pub type Component = ${component}.`);
    if (hasCheck) functions.push(`component_value(value: Component): ${physical(component)} ->\n    value.`);
    payloads.push(`{Atom["css_parts"], List[${physical(component)}]}`);
    const parameters=['first','second','third','fourth'],arities=['single','pair','triple','quad'];
    for(let count=repeated.min;count<=repeated.max;count++){
      const selected=parameters.slice(0,count);
      const fourSides=['margin','padding','inset','border-color','border-width','border-style','scroll-margin','scroll-padding'].includes(property.name);
      const meaning=fourSides?['Applies to all four sides.','First sets top/bottom; second sets left/right.','First sets top; second sets left/right; third sets bottom.','Sets top, right, bottom, and left in that order.'][count-1]:`Supplies ${count} components in grammar order.`;
      functions.push(`${comment(meaning)}\npub ${arities[count-1]}(${selected.map(p=>`${p}: Component`).join(', ')}): Checked ->\n    Checked({Atom["css_parts"], [${selected.map(name=>hasCheck?'component_value('+name+')':name).join(', ')}]}).`);
    }
  }
  if(grouped){
    const parameters=[],tupleTypes=[],tupleValues=[];
    for(const group of grouped.groups){
      const parameter=identifier(group.name),part=`${typeName(group.name)}Part`,check=`Checked${typeName(group.name)}`;
      const hasCheck=checked(group.atoms,check,identifier(group.name)+'_');
      const allowed=choices(group.atoms,hasCheck?check:null).join(' | ');
      declarations.push(`pub type ${part} = ${allowed}.`);
      parameters.push(`${parameter}: ${part}`);tupleTypes.push(physical(allowed));
      if (hasCheck) functions.push(`${parameter}_value(value: ${part}): ${physical(allowed)} ->\n    value.`);
      tupleValues.push(hasCheck?`${parameter}_value(${parameter})`:parameter);
    }
    payloads.push(`{Atom["css_parts"], {${tupleTypes.join(', ')}}}`);
    functions.push(`${comment(`Combines ${grouped.groups.map(g=>g.name).join(', ')} for ${property.name}. Each parameter accepts its own keyword or typed unit choices directly.`)}\npub compose(${parameters.join(', ')}): Checked ->\n    Checked({Atom["css_parts"], {${tupleValues.join(', ')}}}).`);
  }
  if(payloads.length)declarations.push(`pub opaque type Checked = ${unique(payloads).join(' | ')}.`);
  const fieldChoices=unique([...choices(atoms,null),...globals,...(payloads.length?['Checked']:[])]);
  for(const name of globals)types.add(name);
  if(types.size)imports.add(`import type std.css.Values.{${[...types].sort().join(', ')}}.`);
  if(helpers.size)imports.add(`import std.css.Values.{${[...helpers].sort().join(', ')}}.`);
  const meanings=keywordModule?keywords.filter(k=>docs?.values[k]).map(k=>`- ${k}: ${docs.values[k]}${docs.valueSources?.[k]?' See '+docs.valueSources[k]:''}`).join('\n'):'';
  const overview=[docs?.description,propertyFacts(property.name),meanings&&'Keyword effects:\n'+meanings,`Grammar: ${property.syntax}`,`Specification: ${property.href}`,...(docs?[`Author guide: ${docs.source}`,...(docs.license?['Purpose and value descriptions adapted from MDN contributors (CC-BY-SA-2.5).']:[]),...(docs.microsoftValues?['Additional value descriptions: Microsoft (MIT).']:[])]:[]),'Generated from @webref/css 8.7.8. Do not edit.'].filter(Boolean).join('\n\n');
  const source=`${comment(overview)}\nmodule std.css.properties.${name}.\n\n${[...imports].sort().join('\n')}\n\n${declarations.length?declarations.join('\n\n')+'\n\n':''}pub struct Property {\n    ${identifier(property.name)}: ${fieldChoices.join(' | ')},\n}.\n${functions.length?'\n'+functions.join('\n\n')+'\n':''}`;
  return {source,atoms,repeated,grouped,keywords};
}
