"""Offline calculator and allowlisted Windows Settings shortcuts."""
import ast
import math
import operator
import re

SETTINGS = (
    ('Bluetooth','bluetooth','bluetooth devices pairing'),
    ('Wi-Fi','network-wifi','wifi wi-fi wireless internet'),
    ('Network','network-status','network internet ethernet connection'),
    ('Sound','sound','sound audio volume speakers microphone'),
    ('Display','display','display screen monitors resolution brightness'),
    ('Windows Update','windowsupdate','windows update updates'),
    ('Installed apps','appsfeatures','installed apps uninstall programs applications'),
    ('Default apps','defaultapps','default apps browser associations'),
    ('Startup apps','startupapps','startup apps login sign-in'),
    ('Storage','storagesense','storage disk space drives cleanup'),
    ('Mouse','mousetouchpad','mouse pointer touchpad'),
    ('Keyboard','typing','keyboard typing input text'),
    ('Printers and scanners','printers','printers printer scanners scanner'),
    ('Personalization','personalization','personalization wallpaper background colors themes'),
    ('Taskbar','taskbar','taskbar icons task bar'),
    ('Accessibility','easeofaccess','accessibility ease of access'),
    ('Privacy','privacy','privacy permissions'),
    ('Date and time','dateandtime','date time clock timezone time zone'),
    ('Power and battery','powersleep','power battery sleep energy'),
    ('Windows Settings','','windows settings control panel'),
)
SETTING_URIS={'ms-settings:'+uri for _,uri,_ in SETTINGS}
BINARY={ast.Add:operator.add,ast.Sub:operator.sub,ast.Mult:operator.mul,ast.Div:operator.truediv,ast.FloorDiv:operator.floordiv,ast.Mod:operator.mod,ast.Pow:operator.pow}
FUNCTIONS={'sqrt':math.sqrt,'abs':abs,'round':round,'min':min,'max':max}

def calculate(expression):
    if len(expression)>256:raise ValueError('Expression too long')
    expression=expression.replace('×','*').replace('÷','/').replace('^','**')
    expression=re.sub(r'(\d+(?:\.\d+)?)\s*%\s+of\s+',r'(\1/100)*',expression,flags=re.I)
    expression=re.sub(r'(\d+(?:\.\d+)?)\s*%(?=\s*(?:$|[+*/()\-]))',r'(\1/100)',expression)
    tree=ast.parse(expression.strip(),mode='eval')
    if sum(1 for _ in ast.walk(tree))>64:raise ValueError('Expression too complex')
    def visit(node):
        if isinstance(node,ast.Constant) and type(node.value) in (int,float):value=node.value
        elif isinstance(node,ast.Name) and node.id in ('pi','e'):value=getattr(math,node.id)
        elif isinstance(node,ast.UnaryOp) and isinstance(node.op,(ast.UAdd,ast.USub)):
            value=visit(node.operand)*(1 if isinstance(node.op,ast.UAdd) else -1)
        elif isinstance(node,ast.BinOp) and type(node.op) in BINARY:
            left,right=visit(node.left),visit(node.right)
            if isinstance(node.op,ast.Pow) and abs(right)>100:raise ValueError('Exponent too large')
            value=BINARY[type(node.op)](left,right)
        elif isinstance(node,ast.Call) and isinstance(node.func,ast.Name) and node.func.id in FUNCTIONS and not node.keywords and 1<=len(node.args)<=4:
            value=FUNCTIONS[node.func.id](*(visit(arg) for arg in node.args))
        else:raise ValueError('Unsupported expression')
        if type(value) not in (int,float) or not math.isfinite(value) or abs(value)>1e100:raise ValueError('Result out of range')
        return value
    value=visit(tree.body)
    return str(value) if isinstance(value,int) else format(value,'.12g')

def exclusive(query,settings):
    return query.strip().lower().startswith(('calc:','settings:')) or settings.get('search_category')=='Windows settings'

def results(query,settings):
    from engine import SearchResult
    query=query.strip();lower=query.casefold()
    calc=lower.startswith('calc:')
    expression=query[5:].strip() if calc else query.lstrip('= ').strip()
    candidate=calc or query.startswith('=') or bool(re.search(r'[+*/^×÷%()]|\d\s*-\s*\d',query))
    date=bool(re.fullmatch(r'\d{4}-\d{2}(?:-\d{2})?',query))
    if candidate and (calc or not date):
        try:answer=calculate(expression)
        except (ValueError,SyntaxError,ArithmeticError,TypeError):
            if calc or query.startswith('='):return [SearchResult(5,'','Check the expression. Use numbers, arithmetic, parentheses, sqrt, abs or round.',kind='calculator_error',symbol='Unable to calculate',query=query,reasons=['Invalid calculation'])]
        else:return [SearchResult(5,'',expression,kind='calculator',symbol=answer,query=query,reasons=['Calculated locally'])]
    if calc:return []
    explicit=lower.startswith('settings:')
    if settings.get('search_category','All files') not in ('All files','Windows settings') and not explicit:return []
    subject=lower[9:].strip() if explicit else lower
    if not explicit and subject not in ('settings','windows settings'):subject=re.sub(r'\b(?:windows\s+)?settings\b','',subject).strip()
    tokens=re.findall(r'[\w-]+',subject)
    if not tokens and not (explicit or settings.get('search_category')=='Windows settings'):return []
    found=[]
    for title,uri,aliases in SETTINGS:
        if all(token in (title+' '+aliases).casefold() for token in tokens):
            found.append(SearchResult(4,'ms-settings:'+uri,'Open Windows Settings',kind='settings',symbol=title,query=query,reasons=['Matches a Windows settings shortcut']))
    return found
